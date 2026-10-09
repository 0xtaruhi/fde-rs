use anyhow::{Result, bail};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

use crate::{
    DeviceDesign, DeviceDesignIndex,
    domain::{BlockRamPin, SiteKind},
};

use super::{
    endpoint::{ResolvedRouteEndpoint, resolve_route_endpoint},
    mapping::{endpoint_sink_nets, endpoint_source_nets, should_route_device_net},
    router::RouteSinkContext,
    types::RouteNode,
};

struct Request {
    net_index: usize,
    exits: Vec<RouteNode>,
}

/// A saturated BRAM crossbar has no spare tracks: every endpoint must own one.
/// Assign it as a group before routing so a flexible pin cannot occupy a track
/// that a constrained pin needs and then stay frozen during incremental routing.
pub(super) fn saturated_bram_escapes(
    context: &mut RouteSinkContext<'_>,
    device: &DeviceDesign,
    index: &DeviceDesignIndex,
) -> Result<Vec<(RouteNode, usize)>> {
    let mut requests = Vec::new();
    for (net_index, net) in device.nets.iter().enumerate() {
        if !should_route_device_net(net) {
            continue;
        }
        let Some(driver) = &net.driver else {
            continue;
        };
        let ResolvedRouteEndpoint::Cell(cell) = resolve_route_endpoint(device, index, driver)
        else {
            continue;
        };
        if cell.site_kind_class() != SiteKind::BlockRam
            || !BlockRamPin::parse(&driver.pin).is_some_and(BlockRamPin::is_data_output)
        {
            continue;
        }
        let sources = endpoint_source_nets(cell, driver, context.wires);
        let mut exits = Vec::new();
        for source in sources {
            let root = RouteNode::new(driver.x, driver.y, source);
            if let Some(graph) = context.tile_context(&root).and_then(|tile| tile.graph) {
                for &arc_index in graph.adjacency(source) {
                    let exit = RouteNode::new(driver.x, driver.y, graph.arcs[arc_index].to);
                    exits.push(context.stitched_components.occupancy_key(&exit));
                }
            }
        }
        exits.sort_unstable();
        exits.dedup();
        if !exits.is_empty() {
            requests.push(Request { net_index, exits });
        }
    }
    for (net_index, net) in device.nets.iter().enumerate() {
        if !should_route_device_net(net) {
            continue;
        }
        let Some(driver) = &net.driver else {
            continue;
        };
        let ResolvedRouteEndpoint::Cell(driver_cell) =
            resolve_route_endpoint(device, index, driver)
        else {
            continue;
        };
        for sink in &net.sinks {
            let ResolvedRouteEndpoint::Cell(cell) = resolve_route_endpoint(device, index, sink)
            else {
                continue;
            };
            if cell.site_kind_class() != SiteKind::BlockRam
                || !matches!(
                    BlockRamPin::parse(&sink.pin),
                    Some(BlockRamPin::DataIn { .. })
                )
            {
                continue;
            }
            let targets = endpoint_sink_nets(Some(driver_cell), cell, sink, context.wires);
            let mut exits = Vec::new();
            for target in targets {
                let node = RouteNode::new(sink.x, sink.y, target);
                if let Some(graph) = context.tile_context(&node).and_then(|tile| tile.graph) {
                    for arc in &graph.arcs {
                        if arc.to == target {
                            let entry = RouteNode::new(sink.x, sink.y, arc.from);
                            exits.push(context.stitched_components.occupancy_key(&entry));
                        }
                    }
                }
            }
            exits.sort_unstable();
            exits.dedup();
            if !exits.is_empty() {
                requests.push(Request { net_index, exits });
            }
        }
    }
    requests.sort_by_key(|request| (request.exits.len(), request.net_index));
    assign_saturated(&requests)
}

fn assign_saturated(requests: &[Request]) -> Result<Vec<(RouteNode, usize)>> {
    let mut users = HashMap::<RouteNode, Vec<usize>>::default();
    for (request_index, request) in requests.iter().enumerate() {
        for &exit in &request.exits {
            users.entry(exit).or_default().push(request_index);
        }
    }
    let mut visited = vec![false; requests.len()];
    let mut assignments = Vec::new();
    for start in 0..requests.len() {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut pending = vec![start];
        let mut group = Vec::new();
        let mut tracks = HashSet::default();
        while let Some(request_index) = pending.pop() {
            group.push(request_index);
            for &exit in &requests[request_index].exits {
                tracks.insert(exit);
                for &next in &users[&exit] {
                    if !visited[next] {
                        visited[next] = true;
                        pending.push(next);
                    }
                }
            }
        }
        // Preserve the existing routing policy when the group has spare exits.
        if group.len() != tracks.len() {
            continue;
        }
        // One net may legally feed several BRAM pins on one track. Leave those
        // multicast groups to the existing shared-tree router.
        let nets = group
            .iter()
            .map(|&i| requests[i].net_index)
            .collect::<HashSet<_>>();
        if nets.len() != group.len() {
            continue;
        }
        group.sort_unstable();
        let mut owners = HashMap::default();
        for request_index in group {
            if !augment(
                request_index,
                requests,
                &mut owners,
                &mut HashSet::default(),
            ) {
                bail!("saturated block RAM escape crossbar has no exclusive endpoint assignment");
            }
        }
        assignments.extend(
            owners
                .into_iter()
                .map(|(exit, request_index)| (exit, requests[request_index].net_index)),
        );
    }
    assignments.sort_unstable();
    Ok(assignments)
}

fn augment(
    request_index: usize,
    requests: &[Request],
    owners: &mut HashMap<RouteNode, usize>,
    visited: &mut HashSet<RouteNode>,
) -> bool {
    for &exit in &requests[request_index].exits {
        if !visited.insert(exit) {
            continue;
        }
        let owner = owners.get(&exit).copied();
        if owner.is_none_or(|other| augment(other, requests, owners, visited)) {
            owners.insert(exit, request_index);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::types::WireInterner;

    #[test]
    fn matching_moves_flexible_pins_and_leaves_unsaturated_groups_unchanged() {
        let mut wires = WireInterner::default();
        let a = RouteNode::new(0, 0, wires.intern("A"));
        let b = RouteNode::new(0, 0, wires.intern("B"));
        let c = RouteNode::new(1, 0, wires.intern("C"));
        let d = RouteNode::new(1, 0, wires.intern("D"));
        let requests = [
            Request {
                net_index: 0,
                exits: vec![a, b],
            },
            Request {
                net_index: 1,
                exits: vec![a],
            },
            Request {
                net_index: 2,
                exits: vec![c, d],
            },
        ];
        assert_eq!(assign_saturated(&requests).unwrap(), vec![(a, 1), (b, 0)]);
    }

    #[test]
    fn multicast_inputs_keep_shared_tree_routing() {
        let mut wires = WireInterner::default();
        let a = RouteNode::new(0, 0, wires.intern("A"));
        let b = RouteNode::new(0, 0, wires.intern("B"));
        let requests = [
            Request {
                net_index: 0,
                exits: vec![a, b],
            },
            Request {
                net_index: 0,
                exits: vec![a],
            },
        ];
        assert!(assign_saturated(&requests).unwrap().is_empty());
    }

    #[test]
    fn impossible_saturated_matching_fails_explicitly() {
        let mut wires = WireInterner::default();
        let a = RouteNode::new(0, 0, wires.intern("A"));
        let b = RouteNode::new(0, 0, wires.intern("B"));
        let c = RouteNode::new(0, 0, wires.intern("C"));
        let requests = [
            Request {
                net_index: 0,
                exits: vec![a],
            },
            Request {
                net_index: 1,
                exits: vec![a],
            },
            Request {
                net_index: 2,
                exits: vec![a, b, c],
            },
        ];
        assert!(
            assign_saturated(&requests)
                .unwrap_err()
                .to_string()
                .contains("no exclusive endpoint assignment")
        );
    }
}
