use anyhow::{Result, bail};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use smallvec::SmallVec;

use super::cost::{route_heuristic, route_transition_cost};
use super::endpoint::{ResolvedRouteEndpoint, resolve_route_endpoint};
use super::guide::{GuideDistances, GuideRouteMode, GuidedRouteNode, OrderedGuide, guide_penalty};
use super::heap::{frontier_heap_pop, frontier_heap_push};
use super::occupancy::{self, ClaimIndex, NegotiationContext, reserve_route_path};
use super::policy::{
    NeighborAvailability, classify_route_net_kind, neighbor_congestion_cost, neighbors,
};

use super::{
    lookup::TileRouteCache,
    mapping::{
        WireSet, endpoint_sink_nets, endpoint_source_nets, should_route_device_net,
        should_skip_unmapped_sink, sink_requires_all_wires,
    },
    types::{
        DeviceRouteImage, DeviceRoutePip, RouteNegotiationStats, RouteNode, RoutedPip,
        SearchParentStep, SearchState, SiteRouteGraphs, WireId, WireInterner,
    },
    wire::tile_distance,
};
use crate::{
    DeviceCell, DeviceDesign, DeviceDesignIndex, DeviceEndpoint, DeviceNet,
    cil::Cil,
    domain::NetOrigin,
    report::{StageReporter, emit_stage_info, emit_stage_progress, emit_stage_warning},
    resource::{
        Arch,
        routing::{
            StitchedComponentDb, build_stitched_components, load_site_route_graphs,
            load_tile_stitch_db,
        },
    },
};

struct LoadedRouteResources {
    wires: WireInterner,
    graphs: SiteRouteGraphs,
    stitched_components: StitchedComponentDb,
}

struct RoutingState {
    routes: Vec<NetRouteArtifacts>,
    claims: ClaimIndex,
    history: occupancy::HistoryTable,
    present_factor: usize,
    hard_block: bool,
    policy_search: SearchScratch<RouteNode, WireId>,
    guided_search: SearchScratch<GuidedRouteNode, (usize, WireId)>,
}

#[derive(Default)]
struct NetRouteArtifacts {
    pips: Vec<DeviceRoutePip>,
    notes: Vec<String>,
    guide_usage: GuideUsageStats,
    failed: bool,
}

struct RouteNotes<'a, 'b> {
    notes: &'a mut Vec<String>,
    failed: &'a mut bool,
    reporter: &'a mut Option<&'b mut dyn StageReporter>,
}

impl RouteNotes<'_, '_> {
    fn push(&mut self, note: String) {
        *self.failed = true;
        push_route_note(self.notes, self.reporter, note);
    }
}

/// Negotiated-congestion schedule: passes cap, per-pass history increment,
/// and the present-sharing factor growth applied after every contended pass.
const MAX_NEGOTIATION_PASSES: usize = 32;
const HISTORY_INCREMENT: usize = 1;
const PRESENT_FACTOR_INITIAL: usize = 2;
const PRESENT_FACTOR_GROWTH: usize = 2;
const PRESENT_FACTOR_CAP: usize = 256;

impl RoutingState {
    fn new(net_count: usize) -> Self {
        Self {
            routes: (0..net_count)
                .map(|_| NetRouteArtifacts::default())
                .collect(),
            claims: ClaimIndex::new(net_count),
            history: occupancy::HistoryTable::default(),
            present_factor: PRESENT_FACTOR_INITIAL,
            hard_block: false,
            policy_search: SearchScratch::default(),
            guided_search: SearchScratch::default(),
        }
    }

    fn rip_up(&mut self, net_index: usize) {
        self.claims.rip_up(net_index);
        self.routes[net_index] = NetRouteArtifacts::default();
    }

    fn clear_routes(&mut self) {
        self.claims.clear();
        self.routes
            .iter_mut()
            .for_each(|route| *route = NetRouteArtifacts::default());
    }
}

struct SearchScratch<Node, Key> {
    frontier: Vec<SearchState<Node, Key>>,
    best_cost: HashMap<Node, usize>,
    parent: HashMap<Node, SearchParentStep<Node>>,
}

impl<Node, Key> Default for SearchScratch<Node, Key> {
    fn default() -> Self {
        Self {
            frontier: Vec::new(),
            best_cost: HashMap::default(),
            parent: HashMap::default(),
        }
    }
}

struct PreparedRouteNet<'a> {
    net_index: usize,
    net: &'a DeviceNet,
    driver: &'a DeviceEndpoint,
    driver_cell: &'a DeviceCell,
    net_kind: RouteNetKind,
    net_origin: NetOrigin,
    roots: Vec<RouteNode>,
    tree_nodes: HashSet<RouteNode>,
    tree_starts: HashSet<RouteNode>,
    tree_start_costs: HashMap<RouteNode, usize>,
    used_pips: HashSet<(usize, usize, WireId, WireId)>,
}

pub fn route_device_design(
    device: &DeviceDesign,
    arch: &Arch,
    arch_path: &std::path::Path,
    cil: &Cil,
) -> Result<DeviceRouteImage> {
    let mut logger = None;
    route_device_design_internal(device, arch, arch_path, cil, &mut logger)
}

pub fn route_device_design_with_reporter(
    device: &DeviceDesign,
    arch: &Arch,
    arch_path: &std::path::Path,
    cil: &Cil,
    reporter: &mut dyn StageReporter,
) -> Result<DeviceRouteImage> {
    route_device_design_internal(device, arch, arch_path, cil, &mut Some(reporter))
}

fn route_device_design_internal(
    device: &DeviceDesign,
    arch: &Arch,
    arch_path: &std::path::Path,
    cil: &Cil,
    reporter: &mut Option<&mut dyn StageReporter>,
) -> Result<DeviceRouteImage> {
    emit_stage_info(reporter, "route", "loading routing resources");
    let mut resources = load_route_resources(arch, arch_path, cil)?;
    let index = DeviceDesignIndex::build(device);
    let mut state = RoutingState::new(device.nets.len());
    let tile_cache = TileRouteCache::build(arch, cil, &resources.graphs);
    let mut context = RouteSinkContext {
        arch,
        stitched_components: &resources.stitched_components,
        tile_cache: &tile_cache,
        wires: &mut resources.wires,
    };

    let net_order = route_net_order(device, &index);
    let routeable_net_total = net_order
        .iter()
        .filter(|&&net_index| should_route_device_net(&device.nets[net_index]))
        .count();
    emit_stage_info(
        reporter,
        "route",
        format!(
            "routing {} routable nets ({} total nets)",
            routeable_net_total,
            device.nets.len()
        ),
    );

    // PathFinder-style negotiation starts with every net, then rips up only
    // nets touching a contended resource. Unaffected routes and claims stay
    // live, so later passes cost O(contended routes), not O(all routes).
    let mut pending = vec![true; device.nets.len()];
    let mut contested_total = 0usize;
    let mut overuse_total = 0usize;
    let mut passes_used = 0usize;
    let mut routed_net_attempts = 0usize;
    let mut converged = false;
    let mut global_notes = Vec::new();
    for pass in 1..=MAX_NEGOTIATION_PASSES {
        passes_used = pass;
        let rerouteable_total = net_order
            .iter()
            .filter(|&&net_index| {
                pending[net_index] && should_route_device_net(&device.nets[net_index])
            })
            .count();
        let progress_interval = (rerouteable_total / 20).max(1);

        // Remove the whole affected set before routing any of it, then restore
        // the repository's stable net order for deterministic results.
        for &net_index in &net_order {
            if pending[net_index] {
                state.rip_up(net_index);
            }
        }
        let mut routed_net_count = 0usize;
        for &net_index in &net_order {
            if !pending[net_index] {
                continue;
            }
            let should_route = should_route_device_net(&device.nets[net_index]);
            route_net(
                &mut context,
                device,
                &index,
                net_index,
                &mut state,
                reporter,
            );
            if should_route {
                routed_net_count += 1;
                routed_net_attempts += 1;
                if routed_net_count == 1
                    || routed_net_count == rerouteable_total
                    || routed_net_count.is_multiple_of(progress_interval)
                {
                    emit_stage_progress(
                        reporter,
                        "route",
                        format!(
                            "pass {pass}: routed {}/{} affected nets ({:.0}%)",
                            routed_net_count,
                            rerouteable_total,
                            (routed_net_count as f64 / rerouteable_total.max(1) as f64) * 100.0
                        ),
                    );
                }
            }
        }

        let contested = state.claims.contested_resources().collect::<Vec<_>>();
        contested_total = contested.len();
        overuse_total = state.claims.overuse_count();

        if contested_total == 0 {
            converged = true;
            emit_stage_info(
                reporter,
                "route",
                format!("negotiation converged after {pass} pass(es): no shared resources"),
            );
            break;
        }

        pending.fill(false);
        for resource in contested {
            for net_index in state.claims.claimant_nets(resource) {
                pending[net_index] = true;
            }
            occupancy::bump_history(&mut state.history, resource, HISTORY_INCREMENT);
        }
        state.present_factor =
            (state.present_factor * PRESENT_FACTOR_GROWTH).min(PRESENT_FACTOR_CAP);
    }

    if !converged {
        push_route_note(
            &mut global_notes,
            reporter,
            format!(
                "Negotiated routing did not fully converge after \
                 {MAX_NEGOTIATION_PASSES} passes ({contested_total} contended, \
                 {overuse_total} overused); \
                 falling back to hard-blocking final pass."
            ),
        );
        // Legalization pass: clear all claims and re-route every net with
        // hard blocking so no two nets share a physical resource in the
        // final result. Nets that cannot find exclusive paths fail here
        // just as they would have under the old single-pass router.
        state.clear_routes();
        state.hard_block = true;
        for &net_index in &net_order {
            route_net(
                &mut context,
                device,
                &index,
                net_index,
                &mut state,
                reporter,
            );
            if should_route_device_net(&device.nets[net_index]) {
                routed_net_attempts += 1;
            }
        }
        state.hard_block = false;
    }

    validate_final_routing(device, &state, context.wires)?;

    let mut guide_usage = GuideUsageStats::default();
    for route in &state.routes {
        guide_usage.merge(&route.guide_usage);
    }
    let guide_summary = guide_usage.summary();
    emit_stage_info(reporter, "route", &guide_summary);

    let mut pips = Vec::new();
    let mut notes = global_notes;
    for net_index in net_order {
        let route = std::mem::take(&mut state.routes[net_index]);
        pips.extend(route.pips);
        notes.extend(route.notes);
    }
    notes.push(guide_summary);

    Ok(DeviceRouteImage {
        pips,
        notes,
        negotiation: RouteNegotiationStats {
            passes_used,
            final_overuse_count: state.claims.overuse_count(),
            routed_net_attempts,
            converged,
        },
    })
}

fn push_route_note(
    notes: &mut Vec<String>,
    reporter: &mut Option<&mut dyn StageReporter>,
    note: String,
) {
    if is_route_warning_note(&note) {
        emit_stage_warning(reporter, "route", note.clone());
    } else {
        emit_stage_info(reporter, "route", note.clone());
    }
    notes.push(note);
}

fn is_route_warning_note(note: &str) -> bool {
    let lowered = note.to_ascii_lowercase();
    lowered.contains("could not find a rust route")
        || lowered.contains("has no routed driver")
        || lowered.contains("not a routable cell")
        || lowered.contains("has no route-source mapping")
        || lowered.contains("has no route-sink mapping")
}

fn load_route_resources(
    arch: &Arch,
    arch_path: &std::path::Path,
    cil: &Cil,
) -> Result<LoadedRouteResources> {
    let mut wires = WireInterner::default();
    let graphs = load_site_route_graphs(arch_path, cil, &mut wires)?;
    let stitch_db = load_tile_stitch_db(arch_path, &mut wires)?;
    let stitched_components = build_stitched_components(&stitch_db, arch, &wires);
    Ok(LoadedRouteResources {
        wires,
        graphs,
        stitched_components,
    })
}

fn route_net_order(device: &DeviceDesign, index: &DeviceDesignIndex) -> Vec<usize> {
    let mut net_order = (0..device.nets.len()).collect::<Vec<_>>();
    net_order.sort_by_key(|&net_index| route_net_order_key(device, index, net_index));
    net_order
}

fn route_net(
    context: &mut RouteSinkContext<'_>,
    device: &DeviceDesign,
    index: &DeviceDesignIndex,
    net_index: usize,
    state: &mut RoutingState,
    reporter: &mut Option<&mut dyn StageReporter>,
) {
    let mut notes = Vec::new();
    let mut failed = false;
    let should_route = should_route_device_net(&device.nets[net_index]);
    let prepared = prepare_route_net(context, device, index, net_index, &mut notes, reporter);
    if let Some(mut prepared) = prepared {
        let mut route_notes = RouteNotes {
            notes: &mut notes,
            failed: &mut failed,
            reporter,
        };
        for sink in ordered_net_sinks(prepared.net, prepared.driver_cell) {
            route_net_sink(
                context,
                device,
                index,
                &mut prepared,
                sink,
                state,
                &mut route_notes,
            );
        }
    } else if should_route {
        failed = true;
    }
    state.routes[net_index].notes = notes;
    state.routes[net_index].failed = failed;
}

fn validate_final_routing(
    device: &DeviceDesign,
    state: &RoutingState,
    wires: &WireInterner,
) -> Result<()> {
    let mut issues = Vec::new();
    let mut contested = state.claims.contested_resources().collect::<Vec<_>>();
    contested.sort_unstable();
    for resource in contested {
        let node = match resource {
            occupancy::RouteResource::Node(node) | occupancy::RouteResource::Sink(node) => node,
        };
        let claimants = state
            .claims
            .claimant_nets(resource)
            .map(|net_index| device.nets[net_index].name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        issues.push(format!(
            "{} at ({}, {}) is claimed by [{}]",
            wires.resolve(node.wire),
            node.x,
            node.y,
            claimants
        ));
    }
    for (net_index, route) in state.routes.iter().enumerate() {
        if route.failed {
            let detail = route.notes.last().map_or("no route detail", String::as_str);
            issues.push(format!(
                "net '{}' is incomplete: {detail}",
                device.nets[net_index].name
            ));
        }
    }
    if issues.is_empty() {
        return Ok(());
    }
    bail!(
        "routing did not produce a legal complete result: {}",
        issues.join("; ")
    )
}

fn prepare_route_net<'a>(
    context: &mut RouteSinkContext<'_>,
    device: &'a DeviceDesign,
    index: &DeviceDesignIndex<'a>,
    net_index: usize,
    notes: &mut Vec<String>,
    reporter: &mut Option<&mut dyn StageReporter>,
) -> Option<PreparedRouteNet<'a>> {
    let net = &device.nets[net_index];
    if !should_route_device_net(net) {
        return None;
    }

    let Some(driver) = net.driver.as_ref() else {
        push_route_note(
            notes,
            reporter,
            format!("Net {} has no routed driver.", net.name),
        );
        return None;
    };

    let driver_cell = match resolve_route_endpoint(device, index, driver) {
        ResolvedRouteEndpoint::Cell(cell) => cell,
        ResolvedRouteEndpoint::Port(port) => {
            push_route_note(
                notes,
                reporter,
                format!(
                    "Net {} driver {} resolves to device port {} and is not a routable cell.",
                    net.name, driver.name, port.port_name
                ),
            );
            return None;
        }
        ResolvedRouteEndpoint::Unknown => {
            push_route_note(
                notes,
                reporter,
                format!(
                    "Net {} driver {} is not a routable cell.",
                    net.name, driver.name
                ),
            );
            return None;
        }
    };

    let source_nets = endpoint_source_nets(driver_cell, driver, context.wires);
    if source_nets.is_empty() {
        push_route_note(
            notes,
            reporter,
            format!(
                "Net {} driver {}:{} has no route-source mapping.",
                net.name, driver.name, driver.pin
            ),
        );
        return None;
    }

    let roots = source_nets
        .iter()
        .copied()
        .map(|wire| RouteNode::new(driver.x, driver.y, wire))
        .collect::<Vec<_>>();
    let tree_nodes = roots.iter().copied().collect::<HashSet<_>>();
    let tree_starts = tree_nodes.clone();
    let tree_start_costs = roots
        .iter()
        .copied()
        .map(|node| (node, 0usize))
        .collect::<HashMap<_, _>>();

    Some(PreparedRouteNet {
        net_index,
        net,
        driver,
        driver_cell,
        net_kind: classify_route_net_kind(driver_cell),
        net_origin: net.origin_kind(),
        roots,
        tree_nodes,
        tree_starts,
        tree_start_costs,
        used_pips: HashSet::default(),
    })
}

fn ordered_net_sinks<'a>(net: &'a DeviceNet, driver_cell: &DeviceCell) -> Vec<&'a DeviceEndpoint> {
    let mut sinks = net.sinks.iter().collect::<Vec<_>>();
    // The sibling C++ router orders sinks by timing criticality rather than
    // prioritizing same-site loads. We do not have the same per-sink
    // timing numbers here, so use longer/farther sinks as a deterministic
    // proxy and let trivial same-site sinks fall later.
    sinks.sort_by_key(|sink| {
        (
            std::cmp::Reverse(net.guide_tiles_for_sink(sink).len()),
            std::cmp::Reverse(tile_distance(driver_cell.x, driver_cell.y, sink.x, sink.y)),
            sink.x,
            sink.y,
            sink.name.as_str(),
            sink.pin.as_str(),
        )
    });
    sinks
}

fn route_net_sink(
    context: &mut RouteSinkContext<'_>,
    device: &DeviceDesign,
    index: &DeviceDesignIndex,
    prepared: &mut PreparedRouteNet<'_>,
    sink: &DeviceEndpoint,
    state: &mut RoutingState,
    route_notes: &mut RouteNotes<'_, '_>,
) {
    let sink_cell = match resolve_route_endpoint(device, index, sink) {
        ResolvedRouteEndpoint::Cell(cell) => cell,
        ResolvedRouteEndpoint::Port(port) => {
            route_notes.push(format!(
                "Net {} sink {} resolves to device port {} and is not a routable cell.",
                prepared.net.name, sink.name, port.port_name
            ));
            return;
        }
        ResolvedRouteEndpoint::Unknown => {
            route_notes.push(format!(
                "Net {} sink {} is not a routable cell.",
                prepared.net.name, sink.name
            ));
            return;
        }
    };

    let sink_nets = endpoint_sink_nets(Some(prepared.driver_cell), sink_cell, sink, context.wires);
    if sink_nets.is_empty() {
        if should_skip_unmapped_sink(Some(prepared.driver_cell), sink_cell, sink) {
            return;
        }
        route_notes.push(format!(
            "Net {} sink {}:{} has no route-sink mapping.",
            prepared.net.name, sink.name, sink.pin
        ));
        return;
    }

    let sink_wire_groups = sink_wire_groups(sink_cell, sink, sink_nets);
    let sink_guide = prepared.net.guide_tiles_for_sink(sink);
    let ordered_guide = OrderedGuide::new(sink_guide);
    let guide_distances = GuideDistances::new(context.arch, sink_guide);

    for sink_wires in sink_wire_groups {
        let spec = SinkRouteSpec {
            criticality: prepared.net.criticality,
            net_kind: prepared.net_kind,
            strict_clock_sink: prepared.net_kind == RouteNetKind::DedicatedClock
                && sink_wires
                    .iter()
                    .all(|wire| context.wires.metadata(*wire).is_clock_sink()),
            ordered_guide: &ordered_guide,
            guide_distances: &guide_distances,
            roots: &prepared.roots,
            tree_nodes: &prepared.tree_nodes,
            tree_starts: &prepared.tree_starts,
            tree_start_costs: &prepared.tree_start_costs,
            sink_x: sink.x,
            sink_y: sink.y,
            sink_wires: sink_wires.as_slice(),
        };

        let negotiation = NegotiationContext {
            claims: &state.claims,
            history: &state.history,
            present_factor: state.present_factor,
            net_index: prepared.net_index,
            net_origin: prepared.net_origin,
            hard_block: state.hard_block,
        };
        let Some((path, guide_mode)) = route_sink(
            context,
            &negotiation,
            &mut state.policy_search,
            &mut state.guided_search,
            &spec,
        ) else {
            route_notes.push(format!(
                "Net {} could not find a Rust route from {}:{} to {}:{}.",
                prepared.net.name, prepared.driver.name, prepared.driver.pin, sink.name, sink.pin
            ));
            continue;
        };
        commit_routed_path(context, prepared, state, guide_mode, path);
    }
}

fn sink_wire_groups(
    sink_cell: &DeviceCell,
    sink: &DeviceEndpoint,
    sink_nets: WireSet,
) -> Vec<WireSet> {
    if sink_requires_all_wires(sink_cell, sink) {
        sink_nets
            .iter()
            .copied()
            .map(|wire| SmallVec::<[WireId; 1]>::from_buf([wire]))
            .collect()
    } else {
        vec![sink_nets]
    }
}

fn commit_routed_path(
    context: &RouteSinkContext<'_>,
    prepared: &mut PreparedRouteNet<'_>,
    state: &mut RoutingState,
    guide_mode: GuideRouteMode,
    path: SinkRoutePath,
) {
    state.routes[prepared.net_index]
        .guide_usage
        .record(guide_mode);
    reserve_route_path(
        context.stitched_components,
        &mut state.claims,
        prepared.net_index,
        prepared.net_origin,
        &path.nodes,
        &path.pips,
    );
    update_tree_state(prepared, &path.nodes);

    for pip in path.pips {
        if prepared.used_pips.insert((pip.x, pip.y, pip.from, pip.to))
            && let Some(materialized) = context.materialize_pip(pip, &prepared.net.name)
        {
            state.routes[prepared.net_index].pips.push(materialized);
        }
    }
}

fn update_tree_state(prepared: &mut PreparedRouteNet<'_>, path_nodes: &[RouteNode]) {
    if let Some((&start, rest)) = path_nodes.split_first() {
        let base_cost = prepared.tree_start_costs.get(&start).copied().unwrap_or(0);
        for (offset, node) in rest
            .iter()
            .copied()
            .take(rest.len().saturating_sub(1))
            .enumerate()
        {
            prepared
                .tree_start_costs
                .entry(node)
                .or_insert(base_cost + offset + 1);
        }
    }
    prepared.tree_starts.extend(
        path_nodes
            .iter()
            .copied()
            .take(path_nodes.len().saturating_sub(1)),
    );
    prepared.tree_nodes.extend(path_nodes.iter().copied());
}

fn route_net_order_key(
    device: &DeviceDesign,
    index: &DeviceDesignIndex,
    net_index: usize,
) -> (u8, u8, usize, usize, usize) {
    let net = &device.nets[net_index];
    if !should_route_device_net(net) {
        return (2, 0, usize::MAX, usize::MAX, net_index);
    }

    let Some(driver) = net.driver.as_ref() else {
        return (1, 0, usize::MAX, usize::MAX, net_index);
    };

    let ResolvedRouteEndpoint::Cell(driver_cell) = resolve_route_endpoint(device, index, driver)
    else {
        return (1, 0, usize::MAX, usize::MAX, net_index);
    };

    let net_kind_rank = match classify_route_net_kind(driver_cell) {
        RouteNetKind::DedicatedClock => 0,
        RouteNetKind::Generic => 1,
    };
    let max_sink_distance = net
        .sinks
        .iter()
        .filter_map(|sink| match resolve_route_endpoint(device, index, sink) {
            ResolvedRouteEndpoint::Cell(sink_cell) => Some(tile_distance(
                driver_cell.x,
                driver_cell.y,
                sink_cell.x,
                sink_cell.y,
            )),
            _ => None,
        })
        .max()
        .unwrap_or(usize::MAX);

    (
        0,
        net_kind_rank,
        net.sinks.len(),
        max_sink_distance,
        net_index,
    )
}

#[derive(Default)]
struct GuideUsageStats {
    ordered: usize,
    strict: usize,
    relaxed: usize,
    fallback: usize,
    unguided: usize,
    dedicated_clock: usize,
}

impl GuideUsageStats {
    fn record(&mut self, mode: GuideRouteMode) {
        match mode {
            GuideRouteMode::Ordered => self.ordered += 1,
            GuideRouteMode::Strict => self.strict += 1,
            GuideRouteMode::Relaxed => self.relaxed += 1,
            GuideRouteMode::Fallback => self.fallback += 1,
            GuideRouteMode::Unguided => self.unguided += 1,
            GuideRouteMode::DedicatedClock => self.dedicated_clock += 1,
        }
    }

    fn merge(&mut self, other: &Self) {
        self.ordered += other.ordered;
        self.strict += other.strict;
        self.relaxed += other.relaxed;
        self.fallback += other.fallback;
        self.unguided += other.unguided;
        self.dedicated_clock += other.dedicated_clock;
    }

    fn summary(&self) -> String {
        format!(
            "Guide usage: ordered={}, strict={}, relaxed={}, fallback={}, unguided={}, dedicated_clock={}.",
            self.ordered,
            self.strict,
            self.relaxed,
            self.fallback,
            self.unguided,
            self.dedicated_clock
        )
    }
}

pub(super) struct RouteSinkContext<'a> {
    pub(super) arch: &'a Arch,
    pub(super) stitched_components: &'a StitchedComponentDb,
    pub(super) tile_cache: &'a TileRouteCache<'a>,
    pub(super) wires: &'a mut WireInterner,
}

impl RouteSinkContext<'_> {
    pub(super) fn tile_context(
        &self,
        node: &RouteNode,
    ) -> Option<&super::lookup::CachedTileRouteContext<'_>> {
        self.tile_cache.for_node(node)
    }

    fn materialize_pip(&self, pip: RoutedPip, net_name: &str) -> Option<DeviceRoutePip> {
        let node = RouteNode::new(pip.x, pip.y, pip.to);
        let tile = self.tile_context(&node)?;
        let graph = tile.graph?;
        let arc = graph.arcs.get(pip.local_arc)?;
        Some(tile.pip(net_name.to_string(), pip.x, pip.y, arc, self.wires))
    }
}

#[derive(Debug, Clone)]
struct SinkRoutePath {
    nodes: Vec<RouteNode>,
    pips: Vec<RoutedPip>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RouteNetKind {
    Generic,
    DedicatedClock,
}

pub(super) struct SinkRouteSpec<'a> {
    pub(super) net_kind: RouteNetKind,
    pub(super) strict_clock_sink: bool,
    pub(super) criticality: f64,
    pub(super) ordered_guide: &'a OrderedGuide,
    pub(super) guide_distances: &'a GuideDistances,
    pub(super) roots: &'a [RouteNode],
    pub(super) tree_nodes: &'a HashSet<RouteNode>,
    pub(super) tree_starts: &'a HashSet<RouteNode>,
    pub(super) tree_start_costs: &'a HashMap<RouteNode, usize>,
    pub(super) sink_x: usize,
    pub(super) sink_y: usize,
    pub(super) sink_wires: &'a [WireId],
}

fn ordered_start_nodes(spec: &SinkRouteSpec<'_>) -> SmallVec<[RouteNode; 8]> {
    let mut nodes = SmallVec::<[RouteNode; 8]>::new();
    if spec.tree_starts.is_empty() {
        nodes.extend_from_slice(spec.roots);
    } else {
        nodes.extend(spec.tree_starts.iter().copied());
    }
    if nodes.len() <= 1 {
        return nodes;
    }
    nodes.sort_unstable_by_key(|node| {
        (
            spec.tree_start_costs.get(node).copied().unwrap_or(0),
            tile_distance(node.x, node.y, spec.sink_x, spec.sink_y),
            node.x,
            node.y,
            node.wire,
        )
    });
    nodes.dedup();
    nodes
}

fn route_sink(
    context: &RouteSinkContext<'_>,
    negotiation: &NegotiationContext<'_>,
    policy_search: &mut SearchScratch<RouteNode, WireId>,
    guided_search: &mut SearchScratch<GuidedRouteNode, (usize, WireId)>,
    spec: &SinkRouteSpec<'_>,
) -> Option<(SinkRoutePath, GuideRouteMode)> {
    if spec.net_kind == RouteNetKind::DedicatedClock {
        return route_sink_with_policy(context, negotiation, policy_search, spec, None)
            .map(|path| (path, GuideRouteMode::DedicatedClock));
    }

    if let Some(path) = route_sink_following_guide(context, negotiation, guided_search, spec) {
        return Some((path, GuideRouteMode::Ordered));
    }

    if spec.guide_distances.is_active() {
        for (max_guide_distance, mode) in [
            (Some(0usize), GuideRouteMode::Strict),
            (Some(1usize), GuideRouteMode::Relaxed),
            (Some(2usize), GuideRouteMode::Relaxed),
            (None, GuideRouteMode::Fallback),
        ] {
            if let Some(path) = route_sink_with_policy(
                context,
                negotiation,
                policy_search,
                spec,
                max_guide_distance,
            ) {
                return Some((path, mode));
            }
        }
        return None;
    }

    route_sink_with_policy(context, negotiation, policy_search, spec, None)
        .map(|path| (path, GuideRouteMode::Unguided))
}

fn route_sink_following_guide(
    context: &RouteSinkContext<'_>,
    negotiation: &NegotiationContext<'_>,
    search: &mut SearchScratch<GuidedRouteNode, (usize, WireId)>,
    spec: &SinkRouteSpec<'_>,
) -> Option<SinkRoutePath> {
    if !spec.ordered_guide.is_active()
        || spec.ordered_guide.len() < 2
        || spec.ordered_guide.last_tile() != Some((spec.sink_x, spec.sink_y))
    {
        return None;
    }

    seed_search(
        search,
        ordered_start_nodes(spec).into_iter().flat_map(|node| {
            let start_cost = spec.tree_start_costs.get(&node).copied().unwrap_or(0);
            spec.ordered_guide
                .indices_for_tile((node.x, node.y))
                .into_iter()
                .map(move |guide_index| {
                    let guided = GuidedRouteNode { node, guide_index };
                    (
                        guided,
                        start_cost,
                        start_cost
                            + spec.ordered_guide.remaining_steps(guide_index)
                            + tile_distance(node.x, node.y, spec.sink_x, spec.sink_y),
                        (guided.guide_index, guided.node.wire),
                    )
                })
        }),
    );

    run_search(
        context,
        spec,
        search,
        |guided| {
            guided.guide_index == spec.ordered_guide.last_index()
                && guided.node.x == spec.sink_x
                && guided.node.y == spec.sink_y
                && spec.sink_wires.contains(&guided.node.wire)
        },
        |guided| guided.node,
        |state, visit| {
            let availability = NeighborAvailability {
                stitched_components: context.stitched_components,
                congestion: negotiation,
                tree_nodes: spec.tree_nodes,
            };
            for (neighbor, local_arc) in neighbors(
                context,
                &state.node.node,
                spec.net_kind,
                spec.strict_clock_sink,
            ) {
                let Some(next_guide_index) = spec.ordered_guide.advance(
                    state.node.guide_index,
                    (state.node.node.x, state.node.node.y),
                    (neighbor.x, neighbor.y),
                ) else {
                    continue;
                };

                let next_node = GuidedRouteNode {
                    node: neighbor,
                    guide_index: next_guide_index,
                };
                let congestion = match neighbor_congestion_cost(
                    &availability,
                    &state.node.node,
                    &neighbor,
                    local_arc,
                ) {
                    super::policy::NeighborCost::Blocked => continue,
                    super::policy::NeighborCost::Free => 0,
                    super::policy::NeighborCost::Contended(c) => c,
                };
                let next_cost = state.cost
                    + route_transition_cost(context, spec, &state.node.node, &neighbor, local_arc)
                    + congestion;
                visit(
                    next_node,
                    local_arc,
                    next_cost,
                    next_cost
                        + spec.ordered_guide.remaining_steps(next_guide_index)
                        + tile_distance(neighbor.x, neighbor.y, spec.sink_x, spec.sink_y),
                    (next_node.guide_index, next_node.node.wire),
                );
            }
        },
    )
}

fn route_sink_with_policy(
    context: &RouteSinkContext<'_>,
    negotiation: &NegotiationContext<'_>,
    search: &mut SearchScratch<RouteNode, WireId>,
    spec: &SinkRouteSpec<'_>,
    max_guide_distance: Option<usize>,
) -> Option<SinkRoutePath> {
    seed_search(
        search,
        ordered_start_nodes(spec).into_iter().map(|node| {
            let start_cost = spec.tree_start_costs.get(&node).copied().unwrap_or(0);
            (node, start_cost, start_cost, node.wire)
        }),
    );

    run_search(
        context,
        spec,
        search,
        |node| {
            node.x == spec.sink_x && node.y == spec.sink_y && spec.sink_wires.contains(&node.wire)
        },
        |node| node,
        |state, visit| {
            let availability = NeighborAvailability {
                stitched_components: context.stitched_components,
                congestion: negotiation,
                tree_nodes: spec.tree_nodes,
            };
            for (neighbor, local_arc) in
                neighbors(context, &state.node, spec.net_kind, spec.strict_clock_sink)
            {
                if let Some(limit) = max_guide_distance
                    && (neighbor.x != state.node.x || neighbor.y != state.node.y)
                    && spec.guide_distances.distance(neighbor.x, neighbor.y) > limit
                {
                    continue;
                }

                let congestion = match neighbor_congestion_cost(
                    &availability,
                    &state.node,
                    &neighbor,
                    local_arc,
                ) {
                    super::policy::NeighborCost::Blocked => continue,
                    super::policy::NeighborCost::Free => 0,
                    super::policy::NeighborCost::Contended(c) => c,
                };
                let next_cost = state.cost
                    + route_transition_cost(context, spec, &state.node, &neighbor, local_arc)
                    + guide_penalty(&state.node, &neighbor, spec.guide_distances)
                    + congestion;
                let priority =
                    next_cost + route_heuristic(context, &neighbor, spec.sink_x, spec.sink_y);
                visit(neighbor, local_arc, next_cost, priority, neighbor.wire);
            }
        },
    )
}

fn seed_search<Node, Key>(
    search: &mut SearchScratch<Node, Key>,
    starts: impl IntoIterator<Item = (Node, usize, usize, Key)>,
) where
    Node: Copy + Eq + Ord + std::hash::Hash,
    Key: Copy + Ord,
{
    let starts = starts.into_iter();
    let (lower, upper) = starts.size_hint();
    let reserve = upper.unwrap_or(lower);
    search.frontier.clear();
    search.best_cost.clear();
    search.parent.clear();
    if search.frontier.capacity() < reserve {
        search
            .frontier
            .reserve(reserve - search.frontier.capacity());
    }
    if search.best_cost.capacity() < reserve {
        search
            .best_cost
            .reserve(reserve - search.best_cost.capacity());
    }
    if search.parent.capacity() < reserve {
        search.parent.reserve(reserve - search.parent.capacity());
    }
    for (node, cost, priority, key) in starts {
        let order = search.frontier.len();
        frontier_heap_push(
            &mut search.frontier,
            SearchState {
                cost,
                priority,
                order,
                key,
                node,
            },
        );
        search.best_cost.entry(node).or_insert(cost);
    }
}

fn run_search<Node, Key>(
    context: &RouteSinkContext<'_>,
    spec: &SinkRouteSpec<'_>,
    search: &mut SearchScratch<Node, Key>,
    is_goal: impl Fn(Node) -> bool,
    route_node_of: impl Fn(Node) -> RouteNode + Copy,
    mut expand: impl FnMut(
        &SearchState<Node, Key>,
        &mut dyn FnMut(Node, Option<usize>, usize, usize, Key),
    ),
) -> Option<SinkRoutePath>
where
    Node: Copy + Eq + Ord + std::hash::Hash,
    Key: Copy + Ord,
{
    let frontier = &mut search.frontier;
    let best_cost = &mut search.best_cost;
    let parent = &mut search.parent;
    let mut next_order = frontier.len();

    while let Some(state) = frontier_heap_pop(frontier) {
        if is_goal(state.node) {
            return Some(reconstruct_search_path(
                context,
                state.node,
                route_node_of,
                |node| parent.get(node).map(|step| (step.previous, step.local_arc)),
            ));
        }

        let Some(current_best) = best_cost.get(&state.node).copied() else {
            continue;
        };
        if state.cost > current_best {
            continue;
        }

        expand(&state, &mut |node, local_arc, cost, priority, key| {
            if cost >= *best_cost.get(&node).unwrap_or(&usize::MAX) {
                return;
            }

            let joins_existing_tree = {
                let neighbor = route_node_of(node);
                spec.tree_nodes.contains(&neighbor) && !spec.roots.contains(&neighbor)
            };
            best_cost.insert(node, cost);
            parent.insert(
                node,
                SearchParentStep {
                    previous: (!joins_existing_tree).then_some(state.node),
                    local_arc: if joins_existing_tree { None } else { local_arc },
                },
            );
            frontier_heap_push(
                frontier,
                SearchState {
                    cost,
                    priority,
                    order: next_order,
                    key,
                    node,
                },
            );
            next_order += 1;
        });
    }

    None
}

fn reconstruct_search_path<Node: Copy>(
    context: &RouteSinkContext<'_>,
    mut current: Node,
    route_node_of: impl Fn(Node) -> RouteNode,
    parent_step_of: impl Fn(&Node) -> Option<(Option<Node>, Option<usize>)>,
) -> SinkRoutePath {
    let mut reversed = Vec::new();
    let mut reversed_nodes = vec![route_node_of(current)];
    while let Some((previous, local_arc)) = parent_step_of(&current) {
        let Some(previous) = previous else {
            break;
        };
        let current_node = route_node_of(current);
        if let Some(arc_index) = local_arc
            && let Some(tile) = context.tile_context(&current_node)
            && let Some(graph) = tile.graph
            && let Some(arc) = graph.arcs.get(arc_index)
        {
            reversed.push(RoutedPip {
                x: current_node.x,
                y: current_node.y,
                from: arc.from,
                to: arc.to,
                local_arc: arc_index,
            });
        }
        current = previous;
        reversed_nodes.push(route_node_of(current));
    }
    reversed.reverse();
    reversed_nodes.reverse();
    SinkRoutePath {
        nodes: reversed_nodes,
        pips: reversed,
    }
}

#[cfg(test)]
mod tests;
