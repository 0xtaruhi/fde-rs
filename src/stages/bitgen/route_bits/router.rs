use anyhow::Result;
use smallvec::SmallVec;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use super::super::device::{
    DeviceCell, DeviceDesign, DeviceDesignIndex, DeviceEndpoint, DevicePort,
};
use super::{
    graph::load_site_route_graphs,
    lookup::route_context_for_node,
    mapping::{
        endpoint_sink_nets, endpoint_source_nets, should_route_device_net,
        should_skip_unmapped_sink,
    },
    stitch::{TileStitchDb, clock_spine_neighbors, load_tile_stitch_db, stitched_neighbors},
    types::{
        DeviceRouteImage, DeviceRoutePip, GlobalState, ParentStep, RouteNode, RoutedPip,
        SiteRouteGraphs, WireId, WireInterner,
    },
    wire::{step_cost, tile_distance},
};
use crate::{cil::Cil, domain::NetOrigin, resource::Arch};

pub fn route_device_design(
    device: &DeviceDesign,
    arch: &Arch,
    arch_path: &std::path::Path,
    cil: &Cil,
) -> Result<DeviceRouteImage> {
    let mut wires = WireInterner::default();
    let graphs = load_site_route_graphs(arch_path, cil, &mut wires)?;
    let stitch_db = load_tile_stitch_db(arch_path, &mut wires)?;
    let index = DeviceDesignIndex::build(device);

    let mut pips = Vec::new();
    let mut notes = Vec::new();
    let mut ordered_guided_sinks = 0usize;
    let mut strict_guided_sinks = 0usize;
    let mut relaxed_guided_sinks = 0usize;
    let mut fallback_guided_sinks = 0usize;
    let mut unguided_sinks = 0usize;
    let mut occupied_route_sinks = HashMap::<(usize, usize, WireId), RouteSinkOwner>::new();
    let mut context = RouteSinkContext {
        arch,
        cil,
        graphs: &graphs,
        stitch_db: &stitch_db,
        wires: &mut wires,
    };

    for (net_index, net) in device.nets.iter().enumerate() {
        if !should_route_device_net(net) {
            continue;
        }
        let Some(driver) = net.driver.as_ref() else {
            notes.push(format!("Net {} has no routed driver.", net.name));
            continue;
        };
        let driver_cell = match resolve_route_endpoint(device, &index, driver) {
            ResolvedRouteEndpoint::Cell(cell) => cell,
            ResolvedRouteEndpoint::Port(port) => {
                notes.push(format!(
                    "Net {} driver {} resolves to device port {} and is not a routable cell.",
                    net.name, driver.name, port.port_name
                ));
                continue;
            }
            ResolvedRouteEndpoint::Unknown => {
                notes.push(format!(
                    "Net {} driver {} is not a routable cell.",
                    net.name, driver.name
                ));
                continue;
            }
        };
        let source_nets = endpoint_source_nets(driver_cell, driver, context.wires);
        if source_nets.is_empty() {
            notes.push(format!(
                "Net {} driver {}:{} has no route-source mapping.",
                net.name, driver.name, driver.pin
            ));
            continue;
        }

        let mut tree = source_nets
            .iter()
            .copied()
            .map(|wire| RouteNode::new(driver.x, driver.y, wire))
            .collect::<Vec<_>>();
        let mut used_pips = HashSet::<(usize, usize, WireId, WireId)>::new();

        for sink in &net.sinks {
            let sink_cell = match resolve_route_endpoint(device, &index, sink) {
                ResolvedRouteEndpoint::Cell(cell) => cell,
                ResolvedRouteEndpoint::Port(port) => {
                    notes.push(format!(
                        "Net {} sink {} resolves to device port {} and is not a routable cell.",
                        net.name, sink.name, port.port_name
                    ));
                    continue;
                }
                ResolvedRouteEndpoint::Unknown => {
                    notes.push(format!(
                        "Net {} sink {} is not a routable cell.",
                        net.name, sink.name
                    ));
                    continue;
                }
            };
            let sink_nets = endpoint_sink_nets(sink_cell, sink, context.wires);
            if sink_nets.is_empty() {
                if should_skip_unmapped_sink(sink_cell, sink) {
                    continue;
                }
                notes.push(format!(
                    "Net {} sink {}:{} has no route-sink mapping.",
                    net.name, sink.name, sink.pin
                ));
                continue;
            }
            let sink_guide = net.guide_tiles_for_sink(sink);
            let ordered_guide = OrderedGuide::new(sink_guide);
            let guide_distances = GuideDistances::new(arch, sink_guide);
            let spec = SinkRouteSpec {
                net_index,
                net_origin: net.origin_kind(),
                ordered_guide: &ordered_guide,
                guide_distances: &guide_distances,
                tree: &tree,
                sink_x: sink.x,
                sink_y: sink.y,
                sink_wires: sink_nets.as_slice(),
            };
            let Some((path, guide_mode)) = route_sink(&mut context, &occupied_route_sinks, &spec)
            else {
                notes.push(format!(
                    "Net {} could not find a Rust route from {}:{} to {}:{}.",
                    net.name, driver.name, driver.pin, sink.name, sink.pin
                ));
                continue;
            };

            match guide_mode {
                GuideRouteMode::Ordered => ordered_guided_sinks += 1,
                GuideRouteMode::Strict => strict_guided_sinks += 1,
                GuideRouteMode::Relaxed => relaxed_guided_sinks += 1,
                GuideRouteMode::Fallback => fallback_guided_sinks += 1,
                GuideRouteMode::Unguided => unguided_sinks += 1,
            }

            reserve_route_sinks(
                &mut occupied_route_sinks,
                net_index,
                net.origin_kind(),
                &path,
            );
            extend_tree(&mut tree, &path);
            for pip in path {
                if used_pips.insert((pip.x, pip.y, pip.from, pip.to))
                    && let Some(materialized) = context.materialize_pip(pip, &net.name)
                {
                    pips.push(materialized);
                }
            }
        }
    }

    notes.push(format!(
        "Guide usage: ordered={}, strict={}, relaxed={}, fallback={}, unguided={}.",
        ordered_guided_sinks,
        strict_guided_sinks,
        relaxed_guided_sinks,
        fallback_guided_sinks,
        unguided_sinks
    ));

    Ok(DeviceRouteImage { pips, notes })
}

enum ResolvedRouteEndpoint<'a> {
    Cell(&'a DeviceCell),
    Port(&'a DevicePort),
    Unknown,
}

fn resolve_route_endpoint<'a>(
    device: &'a DeviceDesign,
    index: &DeviceDesignIndex<'a>,
    endpoint: &DeviceEndpoint,
) -> ResolvedRouteEndpoint<'a> {
    if let Some(cell_id) = index.cell_for_endpoint(endpoint) {
        return ResolvedRouteEndpoint::Cell(index.cell(device, cell_id));
    }
    if let Some(port_id) = index.port_for_endpoint(endpoint) {
        return ResolvedRouteEndpoint::Port(index.port(device, port_id));
    }
    ResolvedRouteEndpoint::Unknown
}

struct RouteSinkContext<'a> {
    arch: &'a Arch,
    cil: &'a Cil,
    graphs: &'a SiteRouteGraphs,
    stitch_db: &'a TileStitchDb,
    wires: &'a mut WireInterner,
}

impl RouteSinkContext<'_> {
    fn materialize_pip(&self, pip: RoutedPip, net_name: &str) -> Option<DeviceRoutePip> {
        let node = RouteNode::new(pip.x, pip.y, pip.to);
        let tile = route_context_for_node(self.arch, self.cil, &node)?;
        let graph = tile.graph(self.graphs)?;
        let arc = graph.arcs.get(pip.local_arc)?;
        Some(tile.pip(net_name.to_string(), pip.x, pip.y, arc, self.wires))
    }
}

fn extend_tree(tree: &mut Vec<RouteNode>, path: &[RoutedPip]) {
    tree.extend(path.iter().map(|pip| RouteNode::new(pip.x, pip.y, pip.to)));
    tree.sort_by_key(|node| (node.x, node.y, node.wire));
    tree.dedup();
}

fn route_sink(
    context: &mut RouteSinkContext<'_>,
    occupied_route_sinks: &HashMap<(usize, usize, WireId), RouteSinkOwner>,
    spec: &SinkRouteSpec<'_>,
) -> Option<(Vec<RoutedPip>, GuideRouteMode)> {
    if let Some(path) = route_sink_following_guide(context, occupied_route_sinks, spec) {
        return Some((path, GuideRouteMode::Ordered));
    }

    if spec.guide_distances.is_active() {
        for (max_guide_distance, mode) in [
            (Some(0usize), GuideRouteMode::Strict),
            (Some(1usize), GuideRouteMode::Relaxed),
            (Some(2usize), GuideRouteMode::Relaxed),
            (None, GuideRouteMode::Fallback),
        ] {
            if let Some(path) =
                route_sink_with_policy(context, occupied_route_sinks, spec, max_guide_distance)
            {
                return Some((path, mode));
            }
        }
        return None;
    }

    route_sink_with_policy(context, occupied_route_sinks, spec, None)
        .map(|path| (path, GuideRouteMode::Unguided))
}

#[derive(Debug, Clone, Copy)]
enum GuideRouteMode {
    Ordered,
    Strict,
    Relaxed,
    Fallback,
    Unguided,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct GuidedRouteNode {
    node: RouteNode,
    guide_index: usize,
}

#[derive(Debug, Clone, Copy)]
struct GuidedState {
    cost: usize,
    priority: usize,
    node: GuidedRouteNode,
}

#[derive(Debug, Clone, Copy)]
struct GuidedParentStep {
    previous: GuidedRouteNode,
    local_arc: Option<usize>,
}

#[derive(Debug, Clone)]
struct OrderedGuide {
    tiles: Vec<(usize, usize)>,
}

struct SinkRouteSpec<'a> {
    net_index: usize,
    net_origin: NetOrigin,
    ordered_guide: &'a OrderedGuide,
    guide_distances: &'a GuideDistances,
    tree: &'a [RouteNode],
    sink_x: usize,
    sink_y: usize,
    sink_wires: &'a [WireId],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RouteSinkOwner {
    net_index: usize,
    origin: NetOrigin,
    from: WireId,
}

impl OrderedGuide {
    fn new(tiles: &[(usize, usize)]) -> Self {
        let mut ordered = Vec::with_capacity(tiles.len());
        for &tile in tiles {
            if ordered.last().copied() != Some(tile) {
                ordered.push(tile);
            }
        }
        Self { tiles: ordered }
    }

    fn is_active(&self) -> bool {
        !self.tiles.is_empty()
    }

    fn last_index(&self) -> usize {
        self.tiles.len().saturating_sub(1)
    }

    fn last_tile(&self) -> Option<(usize, usize)> {
        self.tiles.last().copied()
    }

    fn indices_for_tile(&self, tile: (usize, usize)) -> SmallVec<[usize; 4]> {
        self.tiles
            .iter()
            .enumerate()
            .filter_map(|(index, &candidate)| (candidate == tile).then_some(index))
            .collect()
    }

    fn remaining_steps(&self, index: usize) -> usize {
        self.last_index().saturating_sub(index)
    }

    fn advance(
        &self,
        current_index: usize,
        current_tile: (usize, usize),
        next_tile: (usize, usize),
    ) -> Option<usize> {
        if !self.is_active() || self.tiles.get(current_index).copied()? != current_tile {
            return None;
        }
        if next_tile == current_tile {
            return Some(current_index);
        }
        for next_index in (current_index + 1)..self.tiles.len() {
            if self.tiles[next_index] != next_tile {
                continue;
            }
            if guide_run_is_linear(&self.tiles[current_index..=next_index]) {
                return Some(next_index);
            }
        }
        None
    }
}

impl Eq for GuidedState {}

impl PartialEq for GuidedState {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.cost == other.cost && self.node == other.node
    }
}

impl Ord for GuidedState {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| other.cost.cmp(&self.cost))
            .then_with(|| self.node.guide_index.cmp(&other.node.guide_index))
            .then_with(|| self.node.node.wire.cmp(&other.node.node.wire))
            .then_with(|| self.node.node.x.cmp(&other.node.node.x))
            .then_with(|| self.node.node.y.cmp(&other.node.node.y))
    }
}

impl PartialOrd for GuidedState {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn route_sink_following_guide(
    context: &mut RouteSinkContext<'_>,
    occupied_route_sinks: &HashMap<(usize, usize, WireId), RouteSinkOwner>,
    spec: &SinkRouteSpec<'_>,
) -> Option<Vec<RoutedPip>> {
    if !spec.ordered_guide.is_active()
        || spec.ordered_guide.last_tile() != Some((spec.sink_x, spec.sink_y))
    {
        return None;
    }

    let mut frontier = BinaryHeap::new();
    let mut best_cost = HashMap::<GuidedRouteNode, usize>::new();
    let mut parent = HashMap::<GuidedRouteNode, GuidedParentStep>::new();

    for &node in spec.tree {
        for guide_index in spec.ordered_guide.indices_for_tile((node.x, node.y)) {
            let guided = GuidedRouteNode { node, guide_index };
            let priority = spec.ordered_guide.remaining_steps(guide_index)
                + tile_distance(node.x, node.y, spec.sink_x, spec.sink_y);
            frontier.push(GuidedState {
                cost: 0,
                priority,
                node: guided,
            });
            best_cost.entry(guided).or_insert(0);
        }
    }

    while let Some(state) = frontier.pop() {
        let Some(current_best) = best_cost.get(&state.node).copied() else {
            continue;
        };
        if state.cost > current_best {
            continue;
        }
        if state.node.guide_index == spec.ordered_guide.last_index()
            && state.node.node.x == spec.sink_x
            && state.node.node.y == spec.sink_y
            && spec.sink_wires.contains(&state.node.node.wire)
        {
            return Some(reconstruct_guided_path(context, &parent, state.node));
        }
        for (neighbor, local_arc) in neighbors(context, &state.node.node) {
            if !route_sink_is_available(
                occupied_route_sinks,
                spec.net_index,
                spec.net_origin,
                &state.node.node,
                &neighbor,
                local_arc,
            ) {
                continue;
            }
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
            let next_cost = state.cost
                + route_step_cost(context.wires, &neighbor, local_arc.is_some())
                + sink_entry_penalty(context.wires, &state.node.node, &neighbor);
            if next_cost < *best_cost.get(&next_node).unwrap_or(&usize::MAX) {
                best_cost.insert(next_node, next_cost);
                parent.insert(
                    next_node,
                    GuidedParentStep {
                        previous: state.node,
                        local_arc,
                    },
                );
                frontier.push(GuidedState {
                    priority: next_cost
                        + spec.ordered_guide.remaining_steps(next_guide_index)
                        + tile_distance(neighbor.x, neighbor.y, spec.sink_x, spec.sink_y),
                    cost: next_cost,
                    node: next_node,
                });
            }
        }
    }

    None
}

fn reconstruct_guided_path(
    context: &RouteSinkContext<'_>,
    parent: &HashMap<GuidedRouteNode, GuidedParentStep>,
    mut current: GuidedRouteNode,
) -> Vec<RoutedPip> {
    let mut reversed = Vec::new();
    while let Some(step) = parent.get(&current).copied() {
        if let Some(arc_index) = step.local_arc
            && let Some(tile) = route_context_for_node(context.arch, context.cil, &current.node)
            && let Some(graph) = tile.graph(context.graphs)
            && let Some(arc) = graph.arcs.get(arc_index)
        {
            reversed.push(RoutedPip {
                x: current.node.x,
                y: current.node.y,
                from: arc.from,
                to: arc.to,
                local_arc: arc_index,
            });
        }
        current = step.previous;
    }
    reversed.reverse();
    reversed
}

fn guide_run_is_linear(run: &[(usize, usize)]) -> bool {
    if run.len() < 2 {
        return true;
    }
    let Some(direction) = guide_step(run[0], run[1]) else {
        return false;
    };
    run.windows(2)
        .all(|window| matches!(window, [from, to] if guide_step(*from, *to) == Some(direction)))
}

fn guide_step(from: (usize, usize), to: (usize, usize)) -> Option<(isize, isize)> {
    let dx = to.0 as isize - from.0 as isize;
    let dy = to.1 as isize - from.1 as isize;
    match (dx, dy) {
        (-1 | 1, 0) | (0, -1 | 1) => Some((dx.signum(), dy.signum())),
        _ => None,
    }
}

fn route_sink_with_policy(
    context: &mut RouteSinkContext<'_>,
    occupied_route_sinks: &HashMap<(usize, usize, WireId), RouteSinkOwner>,
    spec: &SinkRouteSpec<'_>,
    max_guide_distance: Option<usize>,
) -> Option<Vec<RoutedPip>> {
    let goals = spec
        .sink_wires
        .iter()
        .copied()
        .map(|wire| RouteNode::new(spec.sink_x, spec.sink_y, wire))
        .collect::<HashSet<_>>();
    let mut frontier = BinaryHeap::new();
    let mut best_cost = HashMap::<RouteNode, usize>::new();
    let mut parent = HashMap::<RouteNode, ParentStep>::new();

    for &node in spec.tree {
        let priority = tile_distance(node.x, node.y, spec.sink_x, spec.sink_y);
        frontier.push(GlobalState {
            cost: 0,
            priority,
            node,
        });
        best_cost.insert(node, 0);
    }

    while let Some(state) = frontier.pop() {
        if goals.contains(&state.node) {
            return Some(reconstruct_path(context, &parent, state.node));
        }
        let Some(current_best) = best_cost.get(&state.node).copied() else {
            continue;
        };
        if state.cost > current_best {
            continue;
        }
        for (neighbor, local_arc) in neighbors(context, &state.node) {
            if !route_sink_is_available(
                occupied_route_sinks,
                spec.net_index,
                spec.net_origin,
                &state.node,
                &neighbor,
                local_arc,
            ) {
                continue;
            }
            if let Some(limit) = max_guide_distance
                && (neighbor.x != state.node.x || neighbor.y != state.node.y)
                && spec.guide_distances.distance(neighbor.x, neighbor.y) > limit
            {
                continue;
            }
            let guide_penalty = guide_penalty(&state.node, &neighbor, spec.guide_distances);
            let next_cost = state.cost
                + route_step_cost(context.wires, &neighbor, local_arc.is_some())
                + sink_entry_penalty(context.wires, &state.node, &neighbor)
                + guide_penalty;
            if next_cost < *best_cost.get(&neighbor).unwrap_or(&usize::MAX) {
                best_cost.insert(neighbor, next_cost);
                parent.insert(
                    neighbor,
                    ParentStep {
                        previous: state.node,
                        local_arc,
                    },
                );
                frontier.push(GlobalState {
                    priority: next_cost
                        + tile_distance(neighbor.x, neighbor.y, spec.sink_x, spec.sink_y),
                    cost: next_cost,
                    node: neighbor,
                });
            }
        }
    }

    None
}

fn route_step_cost(wires: &WireInterner, neighbor: &RouteNode, programmable: bool) -> usize {
    step_cost(wires.resolve(neighbor.wire), programmable)
}

fn sink_entry_penalty(wires: &WireInterner, current: &RouteNode, neighbor: &RouteNode) -> usize {
    let next_name = wires.resolve(neighbor.wire);
    if !next_name.ends_with("_CLK_B") {
        return 0;
    }

    let current_name = wires.resolve(current.wire);
    if current_name.contains("_P") {
        0
    } else if current_name.contains("GCLK")
        || current_name.contains("CLKV")
        || current_name.contains("CLKC")
    {
        2
    } else if current_name.contains("V6")
        || current_name.contains("H6")
        || current_name.starts_with('N')
        || current_name.starts_with('S')
        || current_name.starts_with('E')
        || current_name.starts_with('W')
    {
        24
    } else {
        8
    }
}

fn route_sink_is_available(
    occupied_route_sinks: &HashMap<(usize, usize, WireId), RouteSinkOwner>,
    net_index: usize,
    net_origin: NetOrigin,
    current: &RouteNode,
    neighbor: &RouteNode,
    local_arc: Option<usize>,
) -> bool {
    let Some(_) = local_arc else {
        return true;
    };
    occupied_route_sinks
        .get(&(neighbor.x, neighbor.y, neighbor.wire))
        .map(|owner| {
            owner.net_index == net_index
                || (owner.from == current.wire
                    && (owner.origin == NetOrigin::SyntheticGclk
                        || net_origin == NetOrigin::SyntheticGclk))
        })
        .unwrap_or(true)
}

fn reserve_route_sinks(
    occupied_route_sinks: &mut HashMap<(usize, usize, WireId), RouteSinkOwner>,
    net_index: usize,
    net_origin: NetOrigin,
    path: &[RoutedPip],
) {
    for pip in path {
        occupied_route_sinks
            .entry((pip.x, pip.y, pip.to))
            .or_insert(RouteSinkOwner {
                net_index,
                origin: net_origin,
                from: pip.from,
            });
    }
}

struct GuideDistances {
    width: usize,
    height: usize,
    field: Option<Vec<usize>>,
}

impl GuideDistances {
    fn new(arch: &Arch, guide_tiles: &[(usize, usize)]) -> Self {
        if guide_tiles.is_empty() || arch.width == 0 || arch.height == 0 {
            return Self {
                width: arch.width,
                height: arch.height,
                field: None,
            };
        }

        let size = arch.width.saturating_mul(arch.height);
        let mut field = vec![usize::MAX; size];
        let mut queue = VecDeque::new();
        for &(x, y) in guide_tiles {
            if x >= arch.width || y >= arch.height || arch.tile_at(x, y).is_none() {
                continue;
            }
            let index = y * arch.width + x;
            if field[index] == 0 {
                continue;
            }
            field[index] = 0;
            queue.push_back((x, y));
        }

        while let Some((x, y)) = queue.pop_front() {
            let index = y * arch.width + x;
            let base = field[index];
            for (nx, ny) in tile_neighbors(arch, x, y) {
                let next_index = ny * arch.width + nx;
                if base + 1 < field[next_index] {
                    field[next_index] = base + 1;
                    queue.push_back((nx, ny));
                }
            }
        }

        Self {
            width: arch.width,
            height: arch.height,
            field: Some(field),
        }
    }

    fn distance(&self, x: usize, y: usize) -> usize {
        self.field
            .as_ref()
            .and_then(|field| {
                if x >= self.width || y >= self.height {
                    None
                } else {
                    field.get(y * self.width + x).copied()
                }
            })
            .unwrap_or(0)
    }

    fn is_active(&self) -> bool {
        self.field.is_some()
    }
}

fn guide_penalty(
    current: &RouteNode,
    neighbor: &RouteNode,
    guide_distances: &GuideDistances,
) -> usize {
    if guide_distances.field.is_none() || (current.x == neighbor.x && current.y == neighbor.y) {
        return 0;
    }

    let current_distance = guide_distances.distance(current.x, current.y);
    let next_distance = guide_distances.distance(neighbor.x, neighbor.y);
    let next_distance = if next_distance == usize::MAX {
        32
    } else {
        next_distance.min(32)
    };
    let drift = next_distance.saturating_sub(current_distance.saturating_add(1));
    next_distance.saturating_mul(4) + drift.saturating_mul(6)
}

fn tile_neighbors(arch: &Arch, x: usize, y: usize) -> SmallVec<[(usize, usize); 4]> {
    let mut neighbors = SmallVec::new();
    for (nx, ny) in [
        (x.wrapping_sub(1), y),
        (x + 1, y),
        (x, y.wrapping_sub(1)),
        (x, y + 1),
    ] {
        if nx < arch.width && ny < arch.height && arch.tile_at(nx, ny).is_some() {
            neighbors.push((nx, ny));
        }
    }
    neighbors
}

fn reconstruct_path(
    context: &RouteSinkContext<'_>,
    parent: &HashMap<RouteNode, ParentStep>,
    mut current: RouteNode,
) -> Vec<RoutedPip> {
    let mut reversed = Vec::new();
    while let Some(step) = parent.get(&current).copied() {
        if let Some(arc_index) = step.local_arc
            && let Some(tile) = route_context_for_node(context.arch, context.cil, &current)
            && let Some(graph) = tile.graph(context.graphs)
            && let Some(arc) = graph.arcs.get(arc_index)
        {
            reversed.push(RoutedPip {
                x: current.x,
                y: current.y,
                from: arc.from,
                to: arc.to,
                local_arc: arc_index,
            });
        }
        current = step.previous;
    }
    reversed.reverse();
    reversed
}

fn neighbors(
    context: &mut RouteSinkContext<'_>,
    node: &RouteNode,
) -> SmallVec<[(RouteNode, Option<usize>); 16]> {
    let mut result = SmallVec::new();
    if let Some(tile) = route_context_for_node(context.arch, context.cil, node)
        && let Some(graph) = tile.graph(context.graphs)
        && let Some(indices) = graph.adjacency.get(&node.wire)
    {
        for index in indices {
            let Some(arc) = graph.arcs.get(*index) else {
                continue;
            };
            if should_skip_local_arc(&tile, arc, context.wires) {
                continue;
            }
            result.push((RouteNode::new(node.x, node.y, arc.to), Some(*index)));
        }
    }

    for (next_x, next_y, next_wire) in stitched_neighbors(context.stitch_db, context.arch, node) {
        result.push((RouteNode::new(next_x, next_y, next_wire), None));
    }
    for (next_x, next_y, next_wire) in clock_spine_neighbors(context.arch, context.wires, node) {
        result.push((RouteNode::new(next_x, next_y, next_wire), None));
    }
    result
}

fn should_skip_local_arc(
    tile: &super::lookup::TileRouteContext<'_>,
    arc: &super::types::SiteRouteArc,
    wires: &WireInterner,
) -> bool {
    if tile.site_type != "GSB_LFT" {
        return false;
    }

    let from = wires.resolve(arc.from);
    let to = wires.resolve(arc.to);
    to == "LEFT_O1" && from.starts_with("LEFT_H6") && from.contains("_BUF")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::domain::NetOrigin;

    use super::super::{lookup::TileRouteContext, types::SiteRouteArc};
    use super::{
        OrderedGuide, RouteNode, RouteSinkOwner, WireInterner, route_sink_is_available,
        should_skip_local_arc, sink_entry_penalty,
    };

    #[test]
    fn ordered_guide_allows_long_span_progress_along_straight_runs() {
        let guide = OrderedGuide::new(&[
            (16, 31),
            (16, 30),
            (16, 29),
            (16, 28),
            (16, 27),
            (16, 26),
            (16, 25),
        ]);
        assert_eq!(guide.advance(0, (16, 31), (16, 25)), Some(6));
        assert_eq!(guide.advance(0, (16, 31), (16, 29)), Some(2));
        assert_eq!(guide.advance(2, (16, 29), (16, 25)), Some(6));
    }

    #[test]
    fn ordered_guide_rejects_skipping_across_turns() {
        let guide = OrderedGuide::new(&[(3, 3), (3, 4), (4, 4), (5, 4)]);
        assert_eq!(guide.advance(0, (3, 3), (5, 4)), None);
        assert_eq!(guide.advance(1, (3, 4), (5, 4)), Some(3));
    }

    #[test]
    fn route_sink_availability_keeps_configurable_sinks_single_owned() {
        let mut wires = WireInterner::default();
        let current_wire = wires.intern("CURRENT");
        let neighbor_wire = wires.intern("NEXT");
        let other_wire = wires.intern("OTHER");
        let current = RouteNode::new(7, 9, current_wire);
        let neighbor = RouteNode::new(7, 9, neighbor_wire);
        let mut occupied = HashMap::new();

        assert!(route_sink_is_available(
            &occupied,
            1,
            NetOrigin::Logical,
            &current,
            &neighbor,
            Some(0),
        ));

        occupied.insert(
            (7, 9, neighbor_wire),
            RouteSinkOwner {
                net_index: 1,
                origin: NetOrigin::Logical,
                from: current_wire,
            },
        );
        assert!(route_sink_is_available(
            &occupied,
            1,
            NetOrigin::Logical,
            &current,
            &neighbor,
            Some(0),
        ));

        occupied.insert(
            (7, 9, neighbor_wire),
            RouteSinkOwner {
                net_index: 2,
                origin: NetOrigin::Logical,
                from: other_wire,
            },
        );
        assert!(!route_sink_is_available(
            &occupied,
            1,
            NetOrigin::Logical,
            &current,
            &neighbor,
            Some(0),
        ));
        assert!(route_sink_is_available(
            &occupied,
            1,
            NetOrigin::Logical,
            &current,
            &neighbor,
            None,
        ));
    }

    #[test]
    fn synthetic_gclk_owner_can_share_same_programmable_sink_arc() {
        let mut wires = WireInterner::default();
        let current_wire = wires.intern("GCLK_PW");
        let neighbor_wire = wires.intern("GCLK");
        let current = RouteNode::new(34, 27, current_wire);
        let neighbor = RouteNode::new(34, 27, neighbor_wire);
        let occupied = HashMap::from([(
            (34, 27, neighbor_wire),
            RouteSinkOwner {
                net_index: 0,
                origin: NetOrigin::SyntheticGclk,
                from: current_wire,
            },
        )]);

        assert!(route_sink_is_available(
            &occupied,
            1,
            NetOrigin::Logical,
            &current,
            &neighbor,
            Some(0),
        ));
        assert!(route_sink_is_available(
            &occupied,
            2,
            NetOrigin::SyntheticGclk,
            &current,
            &neighbor,
            Some(0),
        ));
    }

    #[test]
    fn synthetic_gclk_sharing_still_requires_matching_source_wire() {
        let mut wires = WireInterner::default();
        let source_wire = wires.intern("GCLK_PW");
        let other_source_wire = wires.intern("OTHER_PW");
        let neighbor_wire = wires.intern("GCLK");
        let current = RouteNode::new(34, 27, other_source_wire);
        let neighbor = RouteNode::new(34, 27, neighbor_wire);
        let occupied = HashMap::from([(
            (34, 27, neighbor_wire),
            RouteSinkOwner {
                net_index: 0,
                origin: NetOrigin::SyntheticGclk,
                from: source_wire,
            },
        )]);

        assert!(!route_sink_is_available(
            &occupied,
            1,
            NetOrigin::Logical,
            &current,
            &neighbor,
            Some(0),
        ));
    }

    #[test]
    fn clock_sink_penalty_prefers_pin_stub_over_long_track_entry() {
        let mut wires = WireInterner::default();
        let clock_sink = RouteNode::new(4, 13, wires.intern("S0_CLK_B"));
        let pin_stub = RouteNode::new(4, 13, wires.intern("N_P18"));
        let vertical_track = RouteNode::new(4, 13, wires.intern("V6A2"));

        assert_eq!(sink_entry_penalty(&wires, &pin_stub, &clock_sink), 0);
        assert!(
            sink_entry_penalty(&wires, &vertical_track, &clock_sink)
                > sink_entry_penalty(&wires, &pin_stub, &clock_sink)
        );
    }

    #[test]
    fn blocks_left_h6_buffer_arcs_into_left_o1() {
        let mut wires = WireInterner::default();
        let tile = TileRouteContext {
            tile_name: "LR5",
            tile_type: "LR5",
            site_name: "GSB_LFT",
            site_type: "GSB_LFT",
        };
        let blocked = SiteRouteArc {
            from: wires.intern("LEFT_H6A_BUF1"),
            to: wires.intern("LEFT_O1"),
            basic_cell: "SPS_O1".to_string(),
            bits: Vec::new(),
        };
        let allowed = SiteRouteArc {
            from: wires.intern("LEFT_E_BUF3"),
            to: wires.intern("LEFT_O1"),
            basic_cell: "SPS_O1".to_string(),
            bits: Vec::new(),
        };

        assert!(should_skip_local_arc(&tile, &blocked, &wires));
        assert!(!should_skip_local_arc(&tile, &allowed, &wires));
    }
}
