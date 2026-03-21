use crate::{
    cil::Cil,
    device::{DeviceDesign, DeviceNet},
    ir::RoutePip,
    resource::Arch,
};
use anyhow::{Result, anyhow};
use smallvec::SmallVec;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, VecDeque};

use super::{
    graph::load_site_route_graphs,
    lookup::{cil_tile_site, tile_name_for_node, tile_type_for_node},
    mapping::{
        endpoint_sink_nets, endpoint_source_nets, should_route_device_net,
        should_skip_unmapped_sink,
    },
    stitch::{clock_spine_neighbors, stitched_neighbors},
    types::{
        DeviceRouteImage, DeviceRoutePip, GlobalState, ParentStep, RouteBit, RouteNode,
        SiteRouteGraph,
    },
    wire::tile_distance,
};

pub fn route_device_design(
    device: &DeviceDesign,
    arch: &Arch,
    arch_path: &std::path::Path,
    cil: &Cil,
) -> Result<DeviceRouteImage> {
    let graphs = load_site_route_graphs(arch_path, cil)?;
    let cells = device
        .cells
        .iter()
        .map(|cell| (cell.cell_name.as_str(), cell))
        .collect::<BTreeMap<_, _>>();
    let context = RouteSinkContext {
        arch,
        cil,
        graphs: &graphs,
    };

    let mut pips = Vec::new();
    let mut notes = Vec::new();
    let mut strict_guided_sinks = 0usize;
    let mut relaxed_guided_sinks = 0usize;
    let mut fallback_guided_sinks = 0usize;
    let mut unguided_sinks = 0usize;
    let mut occupied_nodes = BTreeMap::<(usize, usize, String), String>::new();

    for net in &device.nets {
        if !should_route_device_net(net) {
            continue;
        }
        if !net.route_pips.is_empty() {
            pips.extend(materialize_exact_route_pips(
                &context,
                net,
                &mut occupied_nodes,
                &mut notes,
            )?);
            continue;
        }
        let Some(driver) = net.driver.as_ref() else {
            notes.push(format!("Net {} has no routed driver.", net.name));
            continue;
        };
        let Some(driver_cell) = cells.get(driver.name.as_str()).copied() else {
            notes.push(format!(
                "Net {} driver {} is not a routable cell.",
                net.name, driver.name
            ));
            continue;
        };
        let source_nets = endpoint_source_nets(driver_cell, driver);
        if source_nets.is_empty() {
            notes.push(format!(
                "Net {} driver {}:{} has no route-source mapping.",
                net.name, driver.name, driver.pin
            ));
            continue;
        }

        let mut tree = source_nets
            .iter()
            .map(|net_name| RouteNode {
                x: driver.x,
                y: driver.y,
                net: net_name.clone(),
            })
            .collect::<Vec<_>>();
        for node in &tree {
            if !claim_route_node(&mut occupied_nodes, node, &net.name) {
                notes.push(format!(
                    "Net {} source node {}:{}:{} conflicts with an already routed net.",
                    net.name, node.x, node.y, node.net
                ));
            }
        }
        let mut used_pips = BTreeSet::<(usize, usize, String, String)>::new();
        for sink in &net.sinks {
            let Some(sink_cell) = cells.get(sink.name.as_str()).copied() else {
                notes.push(format!(
                    "Net {} sink {} is not a routable cell.",
                    net.name, sink.name
                ));
                continue;
            };
            let sink_nets = endpoint_sink_nets(sink_cell, sink);
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
            let guide_tiles = net.guide_tiles_for_sink(sink);
            let Some((path, guide_mode)) = route_sink(
                &context,
                guide_tiles,
                &tree,
                sink.x,
                sink.y,
                &sink_nets,
                &occupied_nodes,
                &net.name,
            ) else {
                notes.push(format!(
                    "Net {} could not find a Rust route from {}:{} to {}:{}.",
                    net.name, driver.name, driver.pin, sink.name, sink.pin
                ));
                continue;
            };

            match guide_mode {
                GuideRouteMode::Strict => strict_guided_sinks += 1,
                GuideRouteMode::Relaxed => relaxed_guided_sinks += 1,
                GuideRouteMode::Fallback => fallback_guided_sinks += 1,
                GuideRouteMode::Unguided => unguided_sinks += 1,
            }

            claim_route_path(&mut occupied_nodes, &path, &net.name);
            extend_tree(&mut tree, &path);
            for pip in path {
                if used_pips.insert((pip.x, pip.y, pip.from_net.clone(), pip.to_net.clone())) {
                    pips.push(DeviceRoutePip {
                        net_name: net.name.clone(),
                        ..pip
                    });
                }
            }
        }
    }

    notes.push(format!(
        "Guide usage: strict={}, relaxed={}, fallback={}, unguided={}.",
        strict_guided_sinks, relaxed_guided_sinks, fallback_guided_sinks, unguided_sinks
    ));

    Ok(DeviceRouteImage { pips, notes })
}

struct RouteSinkContext<'a> {
    arch: &'a Arch,
    cil: &'a Cil,
    graphs: &'a BTreeMap<String, SiteRouteGraph>,
}

fn materialize_exact_route_pips(
    context: &RouteSinkContext<'_>,
    net: &DeviceNet,
    occupied_nodes: &mut BTreeMap<(usize, usize, String), String>,
    notes: &mut Vec<String>,
) -> Result<Vec<DeviceRoutePip>> {
    let mut resolved = Vec::new();
    let mut used_pips = BTreeSet::<(usize, usize, String, String)>::new();
    for route_pip in &net.route_pips {
        let resolved_pip = resolve_exact_route_pip(context, route_pip, &net.name)?;
        for route_node in [
            RouteNode {
                x: resolved_pip.x,
                y: resolved_pip.y,
                net: resolved_pip.from_net.clone(),
            },
            RouteNode {
                x: resolved_pip.x,
                y: resolved_pip.y,
                net: resolved_pip.to_net.clone(),
            },
        ] {
            if !claim_route_node(occupied_nodes, &route_node, &net.name) {
                return Err(anyhow!(
                    "exact routed net {} reuses physical node {},{},{} owned by another net",
                    net.name,
                    route_node.x,
                    route_node.y,
                    route_node.net
                ));
            }
        }
        let key = (
            resolved_pip.x,
            resolved_pip.y,
            resolved_pip.from_net.clone(),
            resolved_pip.to_net.clone(),
        );
        if used_pips.insert(key) {
            resolved.push(resolved_pip);
        }
    }
    notes.push(format!(
        "Net {} used {} exact routed pip(s) from the input design.",
        net.name,
        resolved.len()
    ));
    Ok(resolved)
}

fn resolve_exact_route_pip(
    context: &RouteSinkContext<'_>,
    route_pip: &RoutePip,
    net_name: &str,
) -> Result<DeviceRoutePip> {
    let node = RouteNode {
        x: route_pip.x,
        y: route_pip.y,
        net: route_pip.to_net.clone(),
    };
    let tile = cil_tile_site(context.arch, context.cil, &node).ok_or_else(|| {
        anyhow!(
            "exact routed net {} references tile {},{} without a CIL transmission site",
            net_name,
            route_pip.x,
            route_pip.y
        )
    })?;
    let graph = context.graphs.get(&tile.site_type).ok_or_else(|| {
        anyhow!(
            "exact routed net {} references unsupported site route graph {} at {},{}",
            net_name,
            tile.site_type,
            route_pip.x,
            route_pip.y
        )
    })?;
    let arc_index = find_local_arc_index(graph, &route_pip.from_net, &route_pip.to_net)
        .ok_or_else(|| {
            anyhow!(
                "exact routed net {} references unknown pip {} -> {} at {},{} ({})",
                net_name,
                route_pip.from_net,
                route_pip.to_net,
                route_pip.x,
                route_pip.y,
                tile.site_type
            )
        })?;
    let mut pip = device_route_pip_from_arc(
        context.arch,
        context.cil,
        context.graphs,
        route_pip.x,
        route_pip.y,
        arc_index,
    )
    .ok_or_else(|| {
        anyhow!(
            "exact routed net {} could not materialize pip {} -> {} at {},{}",
            net_name,
            route_pip.from_net,
            route_pip.to_net,
            route_pip.x,
            route_pip.y
        )
    })?;
    pip.net_name = net_name.to_string();
    Ok(pip)
}

fn find_local_arc_index(graph: &SiteRouteGraph, from_net: &str, to_net: &str) -> Option<usize> {
    graph
        .adjacency
        .get(from_net)?
        .iter()
        .copied()
        .find(|index| {
            graph
                .arcs
                .get(*index)
                .is_some_and(|arc| arc.from == from_net && arc.to == to_net)
        })
}

fn device_route_pip_from_arc(
    arch: &Arch,
    cil: &Cil,
    graphs: &BTreeMap<String, SiteRouteGraph>,
    x: usize,
    y: usize,
    arc_index: usize,
) -> Option<DeviceRoutePip> {
    let node = RouteNode {
        x,
        y,
        net: String::new(),
    };
    let tile = cil_tile_site(arch, cil, &node)?;
    let graph = graphs.get(&tile.site_type)?;
    let arc = graph.arcs.get(arc_index)?;
    Some(DeviceRoutePip {
        net_name: String::new(),
        tile_name: tile_name_for_node(arch, &node).unwrap_or_default(),
        tile_type: tile_type_for_node(arch, &node)
            .unwrap_or_default()
            .to_string(),
        site_name: tile.site_name,
        site_type: tile.site_type,
        x,
        y,
        from_net: arc.from.clone(),
        to_net: arc.to.clone(),
        bits: arc
            .bits
            .iter()
            .map(|bit| RouteBit {
                basic_cell: arc.basic_cell.clone(),
                sram_name: bit.sram_name.clone(),
                value: bit.value,
            })
            .collect(),
    })
}

fn extend_tree(tree: &mut Vec<RouteNode>, path: &[DeviceRoutePip]) {
    let new_nodes = path.iter().map(|pip| RouteNode {
        x: pip.x,
        y: pip.y,
        net: pip.to_net.clone(),
    });
    tree.extend(new_nodes);
    tree.sort_by(|lhs, rhs| {
        (lhs.x, lhs.y, lhs.net.as_str()).cmp(&(rhs.x, rhs.y, rhs.net.as_str()))
    });
    tree.dedup_by(|lhs, rhs| lhs.x == rhs.x && lhs.y == rhs.y && lhs.net == rhs.net);
}

fn route_node_key(node: &RouteNode) -> (usize, usize, String) {
    (node.x, node.y, node.net.clone())
}

fn claim_route_node(
    occupied_nodes: &mut BTreeMap<(usize, usize, String), String>,
    node: &RouteNode,
    net_name: &str,
) -> bool {
    let key = route_node_key(node);
    match occupied_nodes.get(&key) {
        Some(owner) if owner != net_name => false,
        _ => {
            occupied_nodes.insert(key, net_name.to_string());
            true
        }
    }
}

fn claim_route_path(
    occupied_nodes: &mut BTreeMap<(usize, usize, String), String>,
    path: &[DeviceRoutePip],
    net_name: &str,
) {
    for pip in path {
        for net in [&pip.from_net, &pip.to_net] {
            let node = RouteNode {
                x: pip.x,
                y: pip.y,
                net: net.clone(),
            };
            let _ = claim_route_node(occupied_nodes, &node, net_name);
        }
    }
}

fn route_node_is_available(
    occupied_nodes: &BTreeMap<(usize, usize, String), String>,
    node: &RouteNode,
    net_name: &str,
) -> bool {
    occupied_nodes
        .get(&route_node_key(node))
        .is_none_or(|owner| owner == net_name)
}

fn route_sink(
    context: &RouteSinkContext<'_>,
    guide_tiles: &[(usize, usize)],
    tree: &[RouteNode],
    sink_x: usize,
    sink_y: usize,
    sink_nets: &[String],
    occupied_nodes: &BTreeMap<(usize, usize, String), String>,
    net_name: &str,
) -> Option<(Vec<DeviceRoutePip>, GuideRouteMode)> {
    if let Some(ordered_guide) = OrderedGuide::new(guide_tiles)
        && let Some(path) = route_sink_with_ordered_guide(
            context,
            &ordered_guide,
            tree,
            sink_x,
            sink_y,
            sink_nets,
            occupied_nodes,
            net_name,
        )
    {
        return Some((path, GuideRouteMode::Strict));
    }

    let guide_distances = GuideDistances::new(context.arch, guide_tiles);
    if guide_distances.is_active() {
        for (max_guide_distance, mode) in [
            (Some(0usize), GuideRouteMode::Strict),
            (Some(1usize), GuideRouteMode::Relaxed),
            (Some(2usize), GuideRouteMode::Relaxed),
            (None, GuideRouteMode::Fallback),
        ] {
            if let Some(path) = route_sink_with_policy(
                context,
                &guide_distances,
                tree,
                sink_x,
                sink_y,
                sink_nets,
                max_guide_distance,
                occupied_nodes,
                net_name,
            ) {
                return Some((path, mode));
            }
        }
        return None;
    }

    route_sink_with_policy(
        context,
        &guide_distances,
        tree,
        sink_x,
        sink_y,
        sink_nets,
        None,
        occupied_nodes,
        net_name,
    )
    .map(|path| (path, GuideRouteMode::Unguided))
}

#[derive(Debug, Clone, Copy)]
enum GuideRouteMode {
    Strict,
    Relaxed,
    Fallback,
    Unguided,
}

#[derive(Debug, Clone)]
struct OrderedGuide {
    tiles: Vec<(usize, usize)>,
    tile_to_index: BTreeMap<(usize, usize), usize>,
}

impl OrderedGuide {
    fn new(guide_tiles: &[(usize, usize)]) -> Option<Self> {
        let mut tiles = Vec::new();
        for &tile in guide_tiles {
            if tiles.last().copied() != Some(tile) {
                tiles.push(tile);
            }
        }
        if tiles.is_empty() {
            return None;
        }
        let tile_to_index = tiles
            .iter()
            .enumerate()
            .map(|(index, &tile)| (tile, index))
            .collect::<BTreeMap<_, _>>();
        Some(Self {
            tiles,
            tile_to_index,
        })
    }

    fn goal_index(&self) -> usize {
        self.tiles.len().saturating_sub(1)
    }

    fn index_of_tile(&self, x: usize, y: usize) -> Option<usize> {
        self.tile_to_index.get(&(x, y)).copied()
    }

    fn remaining_steps(&self, guide_index: usize) -> usize {
        self.goal_index().saturating_sub(guide_index)
    }

    fn advance(
        &self,
        guide_index: usize,
        from_tile: (usize, usize),
        to_tile: (usize, usize),
    ) -> Option<usize> {
        if from_tile == to_tile {
            return Some(guide_index);
        }
        let next_index = self.index_of_tile(to_tile.0, to_tile.1)?;
        (next_index > guide_index).then_some(next_index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct GuidedRouteNode {
    node: RouteNode,
    guide_index: usize,
}

#[derive(Debug, Clone)]
struct GuidedGlobalState {
    cost: usize,
    priority: usize,
    state: GuidedRouteNode,
}

#[derive(Debug, Clone)]
struct GuidedParentStep {
    previous: GuidedRouteNode,
    local_arc: Option<usize>,
}

impl Eq for GuidedGlobalState {}

impl PartialEq for GuidedGlobalState {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.cost == other.cost && self.state == other.state
    }
}

impl Ord for GuidedGlobalState {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| other.cost.cmp(&self.cost))
            .then_with(|| self.state.guide_index.cmp(&other.state.guide_index))
            .then_with(|| self.state.node.net.cmp(&other.state.node.net))
            .then_with(|| self.state.node.x.cmp(&other.state.node.x))
            .then_with(|| self.state.node.y.cmp(&other.state.node.y))
    }
}

impl PartialOrd for GuidedGlobalState {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn route_sink_with_ordered_guide(
    context: &RouteSinkContext<'_>,
    ordered_guide: &OrderedGuide,
    tree: &[RouteNode],
    sink_x: usize,
    sink_y: usize,
    sink_nets: &[String],
    occupied_nodes: &BTreeMap<(usize, usize, String), String>,
    net_name: &str,
) -> Option<Vec<DeviceRoutePip>> {
    let goals = sink_nets
        .iter()
        .map(|net| RouteNode {
            x: sink_x,
            y: sink_y,
            net: net.clone(),
        })
        .collect::<BTreeSet<_>>();
    let mut frontier = BinaryHeap::new();
    let mut best_cost = HashMap::<GuidedRouteNode, usize>::new();
    let mut parent = HashMap::<GuidedRouteNode, GuidedParentStep>::new();

    for node in tree {
        if !route_node_is_available(occupied_nodes, node, net_name) {
            continue;
        }
        let Some(guide_index) = ordered_guide.index_of_tile(node.x, node.y) else {
            continue;
        };
        let state = GuidedRouteNode {
            node: node.clone(),
            guide_index,
        };
        let priority = tile_distance(node.x, node.y, sink_x, sink_y)
            + ordered_guide.remaining_steps(guide_index);
        frontier.push(GuidedGlobalState {
            cost: 0,
            priority,
            state: state.clone(),
        });
        best_cost.entry(state).or_insert(0);
    }

    while let Some(state) = frontier.pop() {
        if state.state.guide_index == ordered_guide.goal_index()
            && goals.contains(&state.state.node)
        {
            return Some(reconstruct_guided_path(
                context.arch,
                context.cil,
                context.graphs,
                &parent,
                state.state,
            ));
        }
        let Some(current_best) = best_cost.get(&state.state).copied() else {
            continue;
        };
        if state.cost > current_best {
            continue;
        }
        let from_tile = (state.state.node.x, state.state.node.y);
        for (neighbor, local_arc) in
            neighbors(context.arch, context.cil, context.graphs, &state.state.node)
        {
            if !route_node_is_available(occupied_nodes, &neighbor, net_name) {
                continue;
            }
            let to_tile = (neighbor.x, neighbor.y);
            let Some(next_guide_index) =
                ordered_guide.advance(state.state.guide_index, from_tile, to_tile)
            else {
                continue;
            };
            let next_state = GuidedRouteNode {
                node: neighbor,
                guide_index: next_guide_index,
            };
            let next_cost = state.cost + 1;
            if next_cost < *best_cost.get(&next_state).unwrap_or(&usize::MAX) {
                best_cost.insert(next_state.clone(), next_cost);
                parent.insert(
                    next_state.clone(),
                    GuidedParentStep {
                        previous: state.state.clone(),
                        local_arc,
                    },
                );
                frontier.push(GuidedGlobalState {
                    priority: next_cost
                        + tile_distance(next_state.node.x, next_state.node.y, sink_x, sink_y)
                        + ordered_guide.remaining_steps(next_state.guide_index),
                    cost: next_cost,
                    state: next_state,
                });
            }
        }
    }

    None
}

fn route_sink_with_policy(
    context: &RouteSinkContext<'_>,
    guide_distances: &GuideDistances,
    tree: &[RouteNode],
    sink_x: usize,
    sink_y: usize,
    sink_nets: &[String],
    max_guide_distance: Option<usize>,
    occupied_nodes: &BTreeMap<(usize, usize, String), String>,
    net_name: &str,
) -> Option<Vec<DeviceRoutePip>> {
    let goals = sink_nets
        .iter()
        .map(|net| RouteNode {
            x: sink_x,
            y: sink_y,
            net: net.clone(),
        })
        .collect::<BTreeSet<_>>();
    let mut frontier = BinaryHeap::new();
    let mut best_cost = HashMap::<RouteNode, usize>::new();
    let mut parent = HashMap::<RouteNode, ParentStep>::new();

    for node in tree {
        if !route_node_is_available(occupied_nodes, node, net_name) {
            continue;
        }
        let priority = tile_distance(node.x, node.y, sink_x, sink_y);
        frontier.push(GlobalState {
            cost: 0,
            priority,
            node: node.clone(),
        });
        best_cost.insert(node.clone(), 0);
    }

    while let Some(state) = frontier.pop() {
        if goals.contains(&state.node) {
            return Some(reconstruct_path(
                context.arch,
                context.cil,
                context.graphs,
                &parent,
                state.node,
            ));
        }
        let Some(current_best) = best_cost.get(&state.node).copied() else {
            continue;
        };
        if state.cost > current_best {
            continue;
        }
        for (neighbor, local_arc) in
            neighbors(context.arch, context.cil, context.graphs, &state.node)
        {
            if !route_node_is_available(occupied_nodes, &neighbor, net_name) {
                continue;
            }
            if let Some(limit) = max_guide_distance
                && (neighbor.x != state.node.x || neighbor.y != state.node.y)
                && guide_distances.distance(neighbor.x, neighbor.y) > limit
            {
                continue;
            }
            let guide_penalty = guide_penalty(&state.node, &neighbor, guide_distances);
            let next_cost = state.cost + 1 + guide_penalty;
            if next_cost < *best_cost.get(&neighbor).unwrap_or(&usize::MAX) {
                best_cost.insert(neighbor.clone(), next_cost);
                parent.insert(
                    neighbor.clone(),
                    ParentStep {
                        previous: state.node.clone(),
                        local_arc,
                    },
                );
                frontier.push(GlobalState {
                    priority: next_cost + tile_distance(neighbor.x, neighbor.y, sink_x, sink_y),
                    cost: next_cost,
                    node: neighbor,
                });
            }
        }
    }

    None
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
    arch: &Arch,
    cil: &Cil,
    graphs: &BTreeMap<String, SiteRouteGraph>,
    parent: &HashMap<RouteNode, ParentStep>,
    mut current: RouteNode,
) -> Vec<DeviceRoutePip> {
    let mut reversed = Vec::new();
    while let Some(step) = parent.get(&current) {
        if let Some(arc_index) = step.local_arc
            && let Some(pip) =
                device_route_pip_from_arc(arch, cil, graphs, current.x, current.y, arc_index)
        {
            reversed.push(pip);
        }
        current = step.previous.clone();
    }
    reversed.reverse();
    reversed
}

fn reconstruct_guided_path(
    arch: &Arch,
    cil: &Cil,
    graphs: &BTreeMap<String, SiteRouteGraph>,
    parent: &HashMap<GuidedRouteNode, GuidedParentStep>,
    mut current: GuidedRouteNode,
) -> Vec<DeviceRoutePip> {
    let mut reversed = Vec::new();
    while let Some(step) = parent.get(&current) {
        if let Some(arc_index) = step.local_arc
            && let Some(pip) = device_route_pip_from_arc(
                arch,
                cil,
                graphs,
                current.node.x,
                current.node.y,
                arc_index,
            )
        {
            reversed.push(pip);
        }
        current = step.previous.clone();
    }
    reversed.reverse();
    reversed
}

fn neighbors(
    arch: &Arch,
    cil: &Cil,
    graphs: &BTreeMap<String, SiteRouteGraph>,
    node: &RouteNode,
) -> SmallVec<[(RouteNode, Option<usize>); 16]> {
    let mut result = SmallVec::new();
    if let Some(tile_site) = cil_tile_site(arch, cil, node)
        && let Some(graph) = graphs.get(&tile_site.site_type)
        && let Some(indices) = graph.adjacency.get(&node.net)
    {
        for index in indices {
            let Some(arc) = graph.arcs.get(*index) else {
                continue;
            };
            if tile_site.site_type == "GSB_CNT"
                && matches!(node.net.as_str(), "S0_XQ" | "S0_YQ")
                && !matches!(
                    arc.to.as_str(),
                    "OUT2" | "OUT3" | "OUT4" | "OUT5" | "OUT6" | "OUT7"
                )
            {
                continue;
            }
            if tile_site.site_type == "GSB_CNT" && node.net == "OUT5" && arc.to == "LLH6" {
                continue;
            }
            result.push((
                RouteNode {
                    x: node.x,
                    y: node.y,
                    net: arc.to.clone(),
                },
                Some(*index),
            ));
        }
    }

    for (next_x, next_y, next_net) in stitched_neighbors(arch, node) {
        result.push((
            RouteNode {
                x: next_x,
                y: next_y,
                net: next_net,
            },
            None,
        ));
    }
    for (next_x, next_y, next_net) in clock_spine_neighbors(arch, node) {
        result.push((
            RouteNode {
                x: next_x,
                y: next_y,
                net: next_net,
            },
            None,
        ));
    }
    result
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{OrderedGuide, RouteNode, claim_route_path, route_node_is_available};
    use crate::DeviceRoutePip;

    #[test]
    fn ordered_guide_allows_forward_progress_and_long_wire_leaps() {
        let guide = OrderedGuide::new(&[(1, 1), (1, 2), (1, 3), (1, 4), (1, 5), (1, 6), (1, 7)])
            .expect("guide");

        assert_eq!(guide.index_of_tile(1, 1), Some(0));
        assert_eq!(guide.advance(0, (1, 1), (1, 1)), Some(0));
        assert_eq!(guide.advance(0, (1, 1), (1, 7)), Some(6));
    }

    #[test]
    fn ordered_guide_rejects_backward_and_off_path_tile_moves() {
        let guide = OrderedGuide::new(&[(2, 2), (2, 3), (2, 4)]).expect("guide");

        assert_eq!(guide.advance(1, (2, 3), (2, 2)), None);
        assert_eq!(guide.advance(1, (2, 3), (3, 3)), None);
    }

    #[test]
    fn claimed_route_paths_reserve_stitched_from_nodes_too() {
        let mut occupied = BTreeMap::new();
        claim_route_path(
            &mut occupied,
            &[DeviceRoutePip {
                x: 4,
                y: 7,
                from_net: "LLV6".to_string(),
                to_net: "V6N2".to_string(),
                ..DeviceRoutePip::default()
            }],
            "clk",
        );

        assert!(!route_node_is_available(
            &occupied,
            &RouteNode {
                x: 4,
                y: 7,
                net: "LLV6".to_string(),
            },
            "q2",
        ));
        assert!(!route_node_is_available(
            &occupied,
            &RouteNode {
                x: 4,
                y: 7,
                net: "V6N2".to_string(),
            },
            "q2",
        ));
    }
}
