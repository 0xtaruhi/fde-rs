mod cost;
mod search;

use crate::{
    analysis::annotate_net_criticality,
    constraints::{
        ConstraintEntry, apply_constraints, ensure_cluster_positions, ensure_port_positions,
    },
    ir::{Design, Endpoint, RouteSegment},
    report::{StageOutput, StageReport},
    resource::Arch,
};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::BTreeMap};

use self::{
    cost::{RouteMetrics, estimate_route_delay, search_profile},
    search::{astar_to_tree, bfs_to_tree, tree_distance_field},
};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RouteMode {
    BreadthFirst,
    Directed,
    TimingDriven,
}

#[derive(Debug, Clone)]
pub struct RouteOptions {
    pub arch: Arch,
    pub constraints: Vec<ConstraintEntry>,
    pub mode: RouteMode,
}

#[derive(Debug, Clone)]
struct NetPoints {
    driver: (usize, usize),
    sinks: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct EdgeKey((usize, usize), (usize, usize));

pub fn run(mut design: Design, options: &RouteOptions) -> Result<StageOutput<Design>> {
    design.stage = "routed".to_string();
    apply_constraints(&mut design, &options.arch, &options.constraints);
    ensure_port_positions(&mut design, &options.arch);
    if !design.clusters.is_empty() {
        ensure_cluster_positions(&design)?;
    }

    if !matches!(options.mode, RouteMode::BreadthFirst) {
        annotate_net_criticality(&mut design);
    }

    let endpoints = (0..design.nets.len())
        .map(|index| net_points(&design, &options.arch, index))
        .collect::<Result<Vec<_>>>()?;
    let (routes, metrics) = route_design(&design, &endpoints, options)?;

    for (net, route) in design.nets.iter_mut().zip(routes) {
        net.route = route;
        net.route_pips.clear();
    }
    let usage = segment_edge_usage(&design.nets);
    for net in &mut design.nets {
        net.estimated_delay_ns =
            estimate_route_delay(&net.route, options.arch.wire_r, options.arch.wire_c)
                + route_congestion_penalty(&net.route, &usage, &options.arch);
    }

    let mut report = StageReport::new("route");
    report.push(format!(
        "Routed {} nets in {:?} mode across {} iteration(s) with {} occupied grid edges and overflow {}.",
        design.nets.len(),
        options.mode,
        metrics.iterations,
        metrics.occupied_edges,
        metrics.overflow,
    ));
    report.push(format!(
        "Routing metrics: max edge usage {}, overflow nets {}, total length {}, history edges {}.",
        metrics.max_edge_usage, metrics.overflow_nets, metrics.total_length, metrics.history_edges,
    ));

    Ok(StageOutput {
        value: design,
        report,
    })
}

fn route_design(
    design: &Design,
    endpoints: &[NetPoints],
    options: &RouteOptions,
) -> Result<(Vec<Vec<RouteSegment>>, RouteMetrics)> {
    let max_iterations = match options.mode {
        RouteMode::BreadthFirst => 1,
        RouteMode::Directed => 8,
        RouteMode::TimingDriven => 10,
    };

    let mut history = BTreeMap::<EdgeKey, f64>::new();
    let mut point_routes = vec![Vec::new(); design.nets.len()];
    let mut usage = BTreeMap::<EdgeKey, usize>::new();
    for net_index in net_order(design, options.mode) {
        let criticality = design.nets[net_index].criticality;
        let route = route_single_net(
            &endpoints[net_index],
            &options.arch,
            options.mode,
            criticality,
            &usage,
            &history,
            0,
        )
        .ok_or_else(|| anyhow!("failed to route net {}", design.nets[net_index].name))?;
        add_route_usage(&route, &mut usage);
        point_routes[net_index] = route;
    }

    let mut net_overflow = net_overflow_counts(&point_routes, &usage, &options.arch);
    let mut best_metrics = route_metrics(
        &point_routes,
        &usage,
        &net_overflow,
        &options.arch,
        history.len(),
        1,
    );
    let mut best_routes = point_routes
        .iter()
        .map(|route| route_points_to_segments(route))
        .collect::<Vec<_>>();

    if matches!(options.mode, RouteMode::BreadthFirst) || best_metrics.overflow == 0 {
        return Ok((best_routes, best_metrics));
    }

    for iteration in 1..max_iterations {
        accumulate_history(&usage, &mut history, &options.arch, iteration);
        let reroute_nets = select_reroute_nets(design, &net_overflow, options.mode);
        if reroute_nets.is_empty() {
            break;
        }

        for net_index in &reroute_nets {
            remove_route_usage(&point_routes[*net_index], &mut usage);
        }

        for net_index in reroute_nets {
            let criticality = design.nets[net_index].criticality;
            let route = route_single_net(
                &endpoints[net_index],
                &options.arch,
                options.mode,
                criticality,
                &usage,
                &history,
                iteration,
            )
            .ok_or_else(|| anyhow!("failed to route net {}", design.nets[net_index].name))?;
            add_route_usage(&route, &mut usage);
            point_routes[net_index] = route;
        }

        net_overflow = net_overflow_counts(&point_routes, &usage, &options.arch);
        let metrics = route_metrics(
            &point_routes,
            &usage,
            &net_overflow,
            &options.arch,
            history.len(),
            iteration + 1,
        );
        if better_metrics(&metrics, &best_metrics) {
            best_routes = point_routes
                .iter()
                .map(|route| route_points_to_segments(route))
                .collect();
            best_metrics = metrics;
        }

        if best_metrics.overflow == 0 {
            break;
        }
    }

    Ok((best_routes, best_metrics))
}

fn route_single_net(
    endpoints: &NetPoints,
    arch: &Arch,
    mode: RouteMode,
    criticality: f64,
    usage: &BTreeMap<EdgeKey, usize>,
    history: &BTreeMap<EdgeKey, f64>,
    iteration: usize,
) -> Option<Vec<Vec<(usize, usize)>>> {
    let grid_len = arch.width.saturating_mul(arch.height);
    if grid_len == 0 {
        return None;
    }

    let mut tree_points = vec![endpoints.driver];
    let mut tree_mask = vec![false; grid_len];
    if let Some(slot) = tree_mask.get_mut(grid_index(endpoints.driver, arch)) {
        *slot = true;
    }
    let mut pending_sinks = endpoints.sinks.clone();

    let mut paths = Vec::new();
    while !pending_sinks.is_empty() {
        let tree_distance = tree_distance_field(&tree_points, arch);
        let index = farthest_sink_from_tree(&pending_sinks, &tree_distance, arch)?;
        let sink = pending_sinks.remove(index);
        let path = match mode {
            RouteMode::BreadthFirst => bfs_to_tree(sink, &tree_mask, arch),
            RouteMode::Directed | RouteMode::TimingDriven => {
                let profile = search_profile(mode, criticality, iteration);
                astar_to_tree(
                    sink,
                    &tree_mask,
                    &tree_distance,
                    arch,
                    usage,
                    history,
                    profile,
                )
            }
        }?;
        for point in &path {
            let index = grid_index(*point, arch);
            if tree_mask.get(index).copied().unwrap_or(false) {
                continue;
            }
            if let Some(slot) = tree_mask.get_mut(index) {
                *slot = true;
            }
            tree_points.push(*point);
        }
        paths.push(path);
    }

    Some(paths)
}

fn route_metrics(
    point_routes: &[Vec<Vec<(usize, usize)>>],
    usage: &BTreeMap<EdgeKey, usize>,
    net_overflow: &[usize],
    arch: &Arch,
    history_edges: usize,
    iterations: usize,
) -> RouteMetrics {
    let overflow = usage
        .iter()
        .map(|(edge, count)| edge_overflow(*count, *edge, arch))
        .sum::<usize>();
    let max_edge_usage = usage.values().copied().max().unwrap_or(0);
    let total_length = point_routes
        .iter()
        .flat_map(|route| route.iter())
        .map(|path| path.len().saturating_sub(1))
        .sum::<usize>();
    let overflow_nets = net_overflow.iter().filter(|count| **count > 0).count();
    RouteMetrics {
        iterations,
        occupied_edges: usage.len(),
        overflow,
        max_edge_usage,
        history_edges,
        total_length,
        overflow_nets,
    }
}

fn net_order(design: &Design, mode: RouteMode) -> Vec<usize> {
    let mut order = (0..design.nets.len()).collect::<Vec<_>>();
    order.sort_by(|lhs, rhs| match mode {
        RouteMode::BreadthFirst => design.nets[*lhs].name.cmp(&design.nets[*rhs].name),
        RouteMode::Directed => design.nets[*rhs]
            .sinks
            .len()
            .cmp(&design.nets[*lhs].sinks.len())
            .then_with(|| design.nets[*lhs].name.cmp(&design.nets[*rhs].name)),
        RouteMode::TimingDriven => design.nets[*rhs]
            .criticality
            .total_cmp(&design.nets[*lhs].criticality)
            .then_with(|| {
                design.nets[*rhs]
                    .sinks
                    .len()
                    .cmp(&design.nets[*lhs].sinks.len())
            })
            .then_with(|| design.nets[*lhs].name.cmp(&design.nets[*rhs].name)),
    });
    order
}

fn reroute_order(design: &Design, net_overflow: &[usize], mode: RouteMode) -> Vec<usize> {
    let mut order = (0..design.nets.len()).collect::<Vec<_>>();
    order.sort_by(|lhs, rhs| {
        net_overflow[*rhs]
            .cmp(&net_overflow[*lhs])
            .then_with(|| match mode {
                RouteMode::BreadthFirst => Ordering::Equal,
                RouteMode::Directed => design.nets[*rhs]
                    .sinks
                    .len()
                    .cmp(&design.nets[*lhs].sinks.len()),
                RouteMode::TimingDriven => design.nets[*lhs]
                    .criticality
                    .total_cmp(&design.nets[*rhs].criticality)
                    .then_with(|| {
                        design.nets[*rhs]
                            .sinks
                            .len()
                            .cmp(&design.nets[*lhs].sinks.len())
                    }),
            })
            .then_with(|| design.nets[*lhs].name.cmp(&design.nets[*rhs].name))
    });
    order
}

fn select_reroute_nets(design: &Design, net_overflow: &[usize], mode: RouteMode) -> Vec<usize> {
    reroute_order(design, net_overflow, mode)
        .into_iter()
        .filter(|index| {
            if net_overflow[*index] == 0 {
                return false;
            }
            match mode {
                RouteMode::TimingDriven => {
                    design.nets[*index].criticality < 0.92 || net_overflow[*index] >= 2
                }
                _ => true,
            }
        })
        .collect()
}

fn net_points(design: &Design, arch: &Arch, net_index: usize) -> Result<NetPoints> {
    let net = &design.nets[net_index];
    let driver = net
        .driver
        .as_ref()
        .ok_or_else(|| anyhow!("net {} has no driver", net.name))?;
    let driver = endpoint_grid_point(driver, design, arch)
        .ok_or_else(|| anyhow!("net {} driver is not placeable", net.name))?;
    let sinks = net
        .sinks
        .iter()
        .filter_map(|sink| endpoint_grid_point(sink, design, arch))
        .collect::<Vec<_>>();
    Ok(NetPoints { driver, sinks })
}

fn endpoint_grid_point(
    endpoint: &Endpoint,
    design: &Design,
    arch: &Arch,
) -> Option<(usize, usize)> {
    match endpoint.kind.as_str() {
        "cell" => {
            let cluster = design.cluster_lookup(&endpoint.name)?;
            Some((cluster.x?, cluster.y?))
        }
        "port" => {
            let port = design
                .ports
                .iter()
                .find(|port| port.name == endpoint.name)?;
            Some((
                port.x.unwrap_or(arch.width / 2),
                port.y.unwrap_or(arch.height / 2),
            ))
        }
        _ => None,
    }
}

fn route_points_to_segments(paths: &[Vec<(usize, usize)>]) -> Vec<RouteSegment> {
    let mut segments = Vec::new();
    for path in paths {
        for window in path.windows(2) {
            segments.push(RouteSegment {
                x0: window[0].0,
                y0: window[0].1,
                x1: window[1].0,
                y1: window[1].1,
            });
        }
    }
    segments
}

#[cfg(test)]
fn points_to_edges(points: &[(usize, usize)]) -> Vec<EdgeKey> {
    points
        .windows(2)
        .map(|window| canonical_edge(window[0], window[1]))
        .collect()
}

fn segment_edge_usage(nets: &[crate::ir::Net]) -> BTreeMap<EdgeKey, usize> {
    let mut usage = BTreeMap::<EdgeKey, usize>::new();
    for net in nets {
        for segment in &net.route {
            let edge = canonical_edge((segment.x0, segment.y0), (segment.x1, segment.y1));
            *usage.entry(edge).or_insert(0) += 1;
        }
    }
    usage
}

fn route_congestion_penalty(
    route: &[RouteSegment],
    usage: &BTreeMap<EdgeKey, usize>,
    arch: &Arch,
) -> f64 {
    route
        .iter()
        .map(|segment| {
            let edge = canonical_edge((segment.x0, segment.y0), (segment.x1, segment.y1));
            let overflow = edge_overflow(usage.get(&edge).copied().unwrap_or(0), edge, arch) as f64;
            if overflow == 0.0 {
                0.0
            } else {
                0.45 * overflow + 0.15 * overflow * overflow
            }
        })
        .sum()
}

fn add_route_usage(route: &[Vec<(usize, usize)>], usage: &mut BTreeMap<EdgeKey, usize>) {
    for path in route {
        for window in path.windows(2) {
            let edge = canonical_edge(window[0], window[1]);
            *usage.entry(edge).or_insert(0) += 1;
        }
    }
}

fn remove_route_usage(route: &[Vec<(usize, usize)>], usage: &mut BTreeMap<EdgeKey, usize>) {
    for path in route {
        for window in path.windows(2) {
            let edge = canonical_edge(window[0], window[1]);
            let mut remove_edge = false;
            if let Some(count) = usage.get_mut(&edge) {
                *count = count.saturating_sub(1);
                remove_edge = *count == 0;
            }
            if remove_edge {
                usage.remove(&edge);
            }
        }
    }
}

fn accumulate_history(
    usage: &BTreeMap<EdgeKey, usize>,
    history: &mut BTreeMap<EdgeKey, f64>,
    arch: &Arch,
    iteration: usize,
) {
    for (edge, count) in usage {
        let overflow = edge_overflow(*count, *edge, arch);
        if overflow > 0 {
            *history.entry(*edge).or_insert(0.0) +=
                overflow as f64 * (1.0 + iteration as f64 * 0.15);
        }
    }
}

fn better_metrics(candidate: &RouteMetrics, incumbent: &RouteMetrics) -> bool {
    candidate.overflow < incumbent.overflow
        || (candidate.overflow == incumbent.overflow
            && candidate.overflow_nets < incumbent.overflow_nets)
        || (candidate.overflow == incumbent.overflow
            && candidate.overflow_nets == incumbent.overflow_nets
            && candidate.total_length < incumbent.total_length)
        || (candidate.overflow == incumbent.overflow
            && candidate.overflow_nets == incumbent.overflow_nets
            && candidate.total_length == incumbent.total_length
            && candidate.occupied_edges < incumbent.occupied_edges)
}

fn net_overflow_counts(
    point_routes: &[Vec<Vec<(usize, usize)>>],
    usage: &BTreeMap<EdgeKey, usize>,
    arch: &Arch,
) -> Vec<usize> {
    point_routes
        .iter()
        .map(|route| {
            route
                .iter()
                .flat_map(|path| path.windows(2))
                .map(|window| canonical_edge(window[0], window[1]))
                .map(|edge| edge_overflow(usage.get(&edge).copied().unwrap_or(0), edge, arch))
                .sum()
        })
        .collect()
}

fn farthest_sink_from_tree(
    sinks: &[(usize, usize)],
    tree_distance: &[usize],
    arch: &Arch,
) -> Option<usize> {
    sinks
        .iter()
        .enumerate()
        .max_by(|lhs, rhs| {
            tree_distance
                .get(grid_index(*lhs.1, arch))
                .copied()
                .unwrap_or(0)
                .cmp(
                    &tree_distance
                        .get(grid_index(*rhs.1, arch))
                        .copied()
                        .unwrap_or(0),
                )
                .then_with(|| lhs.1.cmp(rhs.1))
        })
        .map(|(index, _)| index)
}

fn edge_overflow(count: usize, edge: EdgeKey, arch: &Arch) -> usize {
    count.saturating_sub(edge_capacity(edge, arch))
}

fn edge_capacity(edge: EdgeKey, arch: &Arch) -> usize {
    arch.edge_capacity(edge.0, edge.1)
}

fn canonical_edge(lhs: (usize, usize), rhs: (usize, usize)) -> EdgeKey {
    if lhs <= rhs {
        EdgeKey(lhs, rhs)
    } else {
        EdgeKey(rhs, lhs)
    }
}

#[cfg(test)]
fn manhattan(lhs: (usize, usize), rhs: (usize, usize)) -> usize {
    lhs.0.abs_diff(rhs.0) + lhs.1.abs_diff(rhs.1)
}

fn grid_index(point: (usize, usize), arch: &Arch) -> usize {
    point.1.saturating_mul(arch.width).saturating_add(point.0)
}
