mod cost;
mod search;

use crate::{
    analysis::annotate_net_criticality,
    constraints::{apply_constraints, ensure_cluster_positions, ensure_port_positions},
    ir::{Design, DesignIndex, Endpoint, RouteSegment},
    report::{StageOutput, StageReport},
    resource::{Arch, SharedArch},
};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
#[cfg(test)]
use std::collections::BTreeMap;

use self::{
    cost::{RouteMetrics, estimate_route_delay, search_profile},
    search::{astar_to_tree_dense, bfs_to_tree, tree_distance_field},
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
    pub arch: SharedArch,
    pub constraints: crate::constraints::SharedConstraints,
    pub mode: RouteMode,
}

#[derive(Debug, Clone)]
struct NetPoints {
    driver: GridPoint,
    sinks: Vec<GridPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct GridPoint {
    x: usize,
    y: usize,
}

impl GridPoint {
    const fn new(x: usize, y: usize) -> Self {
        Self { x, y }
    }

    const fn as_tuple(self) -> (usize, usize) {
        (self.x, self.y)
    }
}

impl From<(usize, usize)> for GridPoint {
    fn from(value: (usize, usize)) -> Self {
        Self::new(value.0, value.1)
    }
}

impl From<GridPoint> for (usize, usize) {
    fn from(value: GridPoint) -> Self {
        value.as_tuple()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct EdgeKey(GridPoint, GridPoint);

#[derive(Debug, Clone)]
struct EdgeArray<T> {
    width: usize,
    horizontal: Vec<T>,
    vertical: Vec<T>,
}

impl<T: Clone> EdgeArray<T> {
    fn new(width: usize, height: usize, default: T) -> Self {
        let horizontal = width.saturating_sub(1).saturating_mul(height);
        let vertical = width.saturating_mul(height.saturating_sub(1));
        Self {
            width,
            horizontal: vec![default.clone(); horizontal],
            vertical: vec![default; vertical],
        }
    }
}

impl<T> EdgeArray<T> {
    fn slot(&self, lhs: GridPoint, rhs: GridPoint) -> Option<(bool, usize)> {
        if lhs.x.abs_diff(rhs.x) + lhs.y.abs_diff(rhs.y) != 1 {
            return None;
        }
        if lhs.y == rhs.y {
            let y = lhs.y;
            let x = lhs.x.min(rhs.x);
            return Some((true, y * self.width.saturating_sub(1) + x));
        }
        let x = lhs.x;
        let y = lhs.y.min(rhs.y);
        Some((false, y * self.width + x))
    }

    fn get(&self, lhs: GridPoint, rhs: GridPoint) -> Option<&T> {
        let (horizontal, index) = self.slot(lhs, rhs)?;
        if horizontal {
            self.horizontal.get(index)
        } else {
            self.vertical.get(index)
        }
    }

    fn get_mut(&mut self, lhs: GridPoint, rhs: GridPoint) -> Option<&mut T> {
        let (horizontal, index) = self.slot(lhs, rhs)?;
        if horizontal {
            self.horizontal.get_mut(index)
        } else {
            self.vertical.get_mut(index)
        }
    }
}

impl EdgeArray<usize> {
    #[cfg(test)]
    fn from_sparse(arch: &Arch, sparse: &BTreeMap<EdgeKey, usize>) -> Self {
        let mut dense = Self::new(arch.width, arch.height, 0usize);
        for (edge, value) in sparse {
            if let Some(slot) = dense.get_mut(edge.0, edge.1) {
                *slot = *value;
            }
        }
        dense
    }

    fn increment(&mut self, lhs: GridPoint, rhs: GridPoint) {
        if let Some(slot) = self.get_mut(lhs, rhs) {
            *slot += 1;
        }
    }

    fn decrement(&mut self, lhs: GridPoint, rhs: GridPoint) {
        if let Some(slot) = self.get_mut(lhs, rhs) {
            *slot = slot.saturating_sub(1);
        }
    }

    fn occupied_edges(&self) -> usize {
        self.horizontal.iter().filter(|value| **value > 0).count()
            + self.vertical.iter().filter(|value| **value > 0).count()
    }

    fn max_value(&self) -> usize {
        self.horizontal
            .iter()
            .chain(self.vertical.iter())
            .copied()
            .max()
            .unwrap_or(0)
    }
}

impl EdgeArray<f64> {
    #[cfg(test)]
    fn from_sparse(arch: &Arch, sparse: &BTreeMap<EdgeKey, f64>) -> Self {
        let mut dense = Self::new(arch.width, arch.height, 0.0f64);
        for (edge, value) in sparse {
            if let Some(slot) = dense.get_mut(edge.0, edge.1) {
                *slot = *value;
            }
        }
        dense
    }

    fn nonzero_edges(&self) -> usize {
        self.horizontal.iter().filter(|value| **value > 0.0).count()
            + self.vertical.iter().filter(|value| **value > 0.0).count()
    }
}

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

    let index = design.index();
    let cluster_points = design
        .clusters
        .iter()
        .map(|cluster| cluster.x.zip(cluster.y).map(GridPoint::from))
        .collect::<Vec<_>>();
    let port_points = design
        .ports
        .iter()
        .map(|port| {
            GridPoint::new(
                port.x.unwrap_or(options.arch.width / 2),
                port.y.unwrap_or(options.arch.height / 2),
            )
        })
        .collect::<Vec<_>>();
    let endpoints = design
        .nets
        .iter()
        .map(|net| net_points(net, &index, &cluster_points, &port_points))
        .collect::<Result<Vec<_>>>()?;
    let (routes, metrics) = route_design(&design, &endpoints, &options.arch, options.mode)?;

    for (net, route) in design.nets.iter_mut().zip(routes) {
        net.route = route;
    }
    let usage = segment_edge_usage_dense(&design.nets, &options.arch);
    for net in &mut design.nets {
        net.estimated_delay_ns =
            estimate_route_delay(&net.route, options.arch.wire_r, options.arch.wire_c)
                + route_congestion_penalty_dense(&net.route, &usage, &options.arch);
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
        "Routing metrics: max edge usage {}, overflow nets {}, total length {}, history edges {}, timing cost {:.3}.",
        metrics.max_edge_usage,
        metrics.overflow_nets,
        metrics.total_length,
        metrics.history_edges,
        metrics.timing_cost,
    ));

    Ok(StageOutput {
        value: design,
        report,
    })
}

fn route_design(
    design: &Design,
    endpoints: &[NetPoints],
    arch: &Arch,
    mode: RouteMode,
) -> Result<(Vec<Vec<RouteSegment>>, RouteMetrics)> {
    let max_iterations = match mode {
        RouteMode::BreadthFirst => 1,
        RouteMode::Directed => 8,
        RouteMode::TimingDriven => 10,
    };

    let mut history = EdgeArray::new(arch.width, arch.height, 0.0f64);
    let mut point_routes = vec![Vec::new(); design.nets.len()];
    let mut usage = EdgeArray::new(arch.width, arch.height, 0usize);
    for net_index in net_order(design, mode) {
        let criticality = design.nets[net_index].criticality;
        let route = route_single_net_dense(
            &endpoints[net_index],
            arch,
            mode,
            criticality,
            &usage,
            &history,
            0,
        )
        .ok_or_else(|| anyhow!("failed to route net {}", design.nets[net_index].name))?;
        add_route_usage(&route, &mut usage);
        point_routes[net_index] = route;
    }

    let mut net_overflow = net_overflow_counts(&point_routes, &usage, arch);
    let mut best_metrics = route_metrics(
        design,
        &point_routes,
        &usage,
        &net_overflow,
        arch,
        history.nonzero_edges(),
        1,
    );
    let mut best_point_routes = point_routes.clone();
    let mut best_routes = point_routes
        .iter()
        .map(|route| route_points_to_segments(route))
        .collect::<Vec<_>>();

    if matches!(mode, RouteMode::BreadthFirst) || best_metrics.overflow == 0 {
        return Ok((best_routes, best_metrics));
    }

    for iteration in 1..max_iterations {
        accumulate_history(&usage, &mut history, arch, iteration);
        let reroute_nets =
            select_reroute_nets(design, &point_routes, &usage, &net_overflow, arch, mode);
        if reroute_nets.is_empty() {
            break;
        }

        for net_index in &reroute_nets {
            remove_route_usage(&point_routes[*net_index], &mut usage);
        }

        for net_index in reroute_nets {
            let criticality = design.nets[net_index].criticality;
            let route = route_single_net_dense(
                &endpoints[net_index],
                arch,
                mode,
                criticality,
                &usage,
                &history,
                iteration,
            )
            .ok_or_else(|| anyhow!("failed to route net {}", design.nets[net_index].name))?;
            add_route_usage(&route, &mut usage);
            point_routes[net_index] = route;
        }

        net_overflow = net_overflow_counts(&point_routes, &usage, arch);
        let metrics = route_metrics(
            design,
            &point_routes,
            &usage,
            &net_overflow,
            arch,
            history.nonzero_edges(),
            iteration + 1,
        );
        if better_metrics(&metrics, &best_metrics, mode) {
            best_point_routes = point_routes.clone();
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

    if matches!(mode, RouteMode::TimingDriven) && best_metrics.overflow == 0 {
        let (refined_routes, refined_metrics) = polish_timing_routes(
            design,
            endpoints,
            arch,
            &history,
            max_iterations,
            &best_point_routes,
            best_metrics,
        )?;
        if better_metrics(&refined_metrics, &best_metrics, mode) {
            best_point_routes = refined_routes;
            best_routes = best_point_routes
                .iter()
                .map(|route| route_points_to_segments(route))
                .collect();
            best_metrics = refined_metrics;
        }
    }

    Ok((best_routes, best_metrics))
}

#[cfg(test)]
fn route_single_net(
    endpoints: &NetPoints,
    arch: &Arch,
    mode: RouteMode,
    criticality: f64,
    usage: &BTreeMap<EdgeKey, usize>,
    history: &BTreeMap<EdgeKey, f64>,
    iteration: usize,
) -> Option<Vec<Vec<GridPoint>>> {
    let usage_dense = EdgeArray::<usize>::from_sparse(arch, usage);
    let history_dense = EdgeArray::<f64>::from_sparse(arch, history);
    route_single_net_dense(
        endpoints,
        arch,
        mode,
        criticality,
        &usage_dense,
        &history_dense,
        iteration,
    )
}

fn route_single_net_dense(
    endpoints: &NetPoints,
    arch: &Arch,
    mode: RouteMode,
    criticality: f64,
    usage: &EdgeArray<usize>,
    history: &EdgeArray<f64>,
    iteration: usize,
) -> Option<Vec<Vec<GridPoint>>> {
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
                astar_to_tree_dense(
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
    design: &Design,
    point_routes: &[Vec<Vec<GridPoint>>],
    usage: &EdgeArray<usize>,
    net_overflow: &[usize],
    arch: &Arch,
    history_edges: usize,
    iterations: usize,
) -> RouteMetrics {
    let overflow = usage
        .horizontal
        .iter()
        .enumerate()
        .map(|(index, count)| {
            let edge = horizontal_edge(index, arch.width);
            edge_overflow(*count, edge, arch)
        })
        .sum::<usize>()
        + usage
            .vertical
            .iter()
            .enumerate()
            .map(|(index, count)| {
                let edge = vertical_edge(index, arch.width);
                edge_overflow(*count, edge, arch)
            })
            .sum::<usize>();
    let max_edge_usage = usage.max_value();
    let total_length = point_routes
        .iter()
        .flat_map(|route| route.iter())
        .map(|path| path.len().saturating_sub(1))
        .sum::<usize>();
    let overflow_nets = net_overflow.iter().filter(|count| **count > 0).count();
    let timing_cost = route_timing_cost(design, point_routes, usage, arch);
    RouteMetrics {
        iterations,
        occupied_edges: usage.occupied_edges(),
        overflow,
        max_edge_usage,
        history_edges,
        total_length,
        overflow_nets,
        timing_cost,
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

fn select_reroute_nets(
    design: &Design,
    point_routes: &[Vec<Vec<GridPoint>>],
    usage: &EdgeArray<usize>,
    net_overflow: &[usize],
    arch: &Arch,
    mode: RouteMode,
) -> Vec<usize> {
    let mut selected = reroute_order(design, net_overflow, mode)
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
        .collect::<Vec<_>>();

    if matches!(mode, RouteMode::TimingDriven) {
        let pressure_scores =
            route_pressure_scores(design, point_routes, usage, net_overflow, arch);
        let mut picked = vec![false; design.nets.len()];
        for index in &selected {
            if let Some(slot) = picked.get_mut(*index) {
                *slot = true;
            }
        }
        let extra_budget = design.nets.len().div_ceil(6).clamp(1, 8);
        let mut extras = timing_pressure_order(design, &pressure_scores)
            .into_iter()
            .filter(|index| !picked[*index])
            .filter(|index| {
                let net = &design.nets[*index];
                net.criticality >= 0.55 || pressure_scores[*index] > 0.0
            })
            .take(extra_budget)
            .collect::<Vec<_>>();
        selected.append(&mut extras);
    }

    selected
}

fn polish_timing_routes(
    design: &Design,
    endpoints: &[NetPoints],
    arch: &Arch,
    history: &EdgeArray<f64>,
    iteration: usize,
    point_routes: &[Vec<Vec<GridPoint>>],
    base_metrics: RouteMetrics,
) -> Result<(Vec<Vec<Vec<GridPoint>>>, RouteMetrics)> {
    let mut refined_routes = point_routes.to_vec();
    let mut usage = point_route_usage_dense(&refined_routes, arch);
    let mut net_overflow = net_overflow_counts(&refined_routes, &usage, arch);
    if net_overflow.iter().any(|count| *count > 0) {
        return Ok((refined_routes, base_metrics));
    }

    let polish_iteration = base_metrics.iterations.max(iteration).saturating_add(1);
    let history_edges = history.nonzero_edges();
    let mut metrics = route_metrics(
        design,
        &refined_routes,
        &usage,
        &net_overflow,
        arch,
        history_edges,
        polish_iteration,
    );
    let pressure_scores =
        route_pressure_scores(design, &refined_routes, &usage, &net_overflow, arch);
    let polish_budget = design.nets.len().div_ceil(4).clamp(1, 6);
    for net_index in timing_pressure_order(design, &pressure_scores)
        .into_iter()
        .take(polish_budget)
    {
        let original_route = refined_routes[net_index].clone();
        remove_route_usage(&original_route, &mut usage);
        let Some(candidate_route) = route_single_net_dense(
            &endpoints[net_index],
            arch,
            RouteMode::TimingDriven,
            design.nets[net_index].criticality,
            &usage,
            history,
            polish_iteration,
        ) else {
            add_route_usage(&original_route, &mut usage);
            continue;
        };
        add_route_usage(&candidate_route, &mut usage);
        refined_routes[net_index] = candidate_route;

        net_overflow = net_overflow_counts(&refined_routes, &usage, arch);
        let candidate_metrics = route_metrics(
            design,
            &refined_routes,
            &usage,
            &net_overflow,
            arch,
            history_edges,
            polish_iteration,
        );
        if better_metrics(&candidate_metrics, &metrics, RouteMode::TimingDriven) {
            metrics = candidate_metrics;
            continue;
        }

        remove_route_usage(&refined_routes[net_index], &mut usage);
        add_route_usage(&original_route, &mut usage);
        refined_routes[net_index] = original_route;
    }

    Ok((refined_routes, metrics))
}

fn point_route_usage_dense(point_routes: &[Vec<Vec<GridPoint>>], arch: &Arch) -> EdgeArray<usize> {
    let mut usage = EdgeArray::new(arch.width, arch.height, 0usize);
    for route in point_routes {
        add_route_usage(route, &mut usage);
    }
    usage
}

fn route_timing_cost(
    design: &Design,
    point_routes: &[Vec<Vec<GridPoint>>],
    usage: &EdgeArray<usize>,
    arch: &Arch,
) -> f64 {
    point_routes
        .iter()
        .zip(&design.nets)
        .map(|(route, net)| {
            let weight = 1.0
                + 2.25 * net.criticality.clamp(0.0, 1.0)
                + 0.12 * net.sinks.len().saturating_sub(1) as f64;
            point_route_delay(route, arch) * weight
                + point_route_congestion_penalty_dense(route, usage, arch) * (1.0 + 1.5 * weight)
        })
        .sum()
}

fn route_pressure_scores(
    design: &Design,
    point_routes: &[Vec<Vec<GridPoint>>],
    usage: &EdgeArray<usize>,
    net_overflow: &[usize],
    arch: &Arch,
) -> Vec<f64> {
    point_routes
        .iter()
        .enumerate()
        .map(|(net_index, route)| {
            let net = &design.nets[net_index];
            let criticality = net.criticality.clamp(0.0, 1.0);
            let overflow = net_overflow.get(net_index).copied().unwrap_or(0) as f64;
            let delay = point_route_delay(route, arch);
            let congestion = point_route_congestion_penalty_dense(route, usage, arch);
            overflow * 48.0
                + (delay + congestion) * (1.0 + 2.8 * criticality)
                + net.sinks.len().saturating_sub(1) as f64 * 0.2
        })
        .collect()
}

fn timing_pressure_order(design: &Design, pressure_scores: &[f64]) -> Vec<usize> {
    let mut order = (0..design.nets.len()).collect::<Vec<_>>();
    order.sort_by(|lhs, rhs| {
        pressure_scores[*rhs]
            .total_cmp(&pressure_scores[*lhs])
            .then_with(|| {
                design.nets[*rhs]
                    .criticality
                    .total_cmp(&design.nets[*lhs].criticality)
            })
            .then_with(|| {
                design.nets[*rhs]
                    .sinks
                    .len()
                    .cmp(&design.nets[*lhs].sinks.len())
            })
            .then_with(|| design.nets[*lhs].name.cmp(&design.nets[*rhs].name))
    });
    order
}

fn net_points(
    net: &crate::ir::Net,
    index: &DesignIndex<'_>,
    cluster_points: &[Option<GridPoint>],
    port_points: &[GridPoint],
) -> Result<NetPoints> {
    let driver = net
        .driver
        .as_ref()
        .ok_or_else(|| anyhow!("net {} has no driver", net.name))?;
    let driver = endpoint_grid_point(driver, index, cluster_points, port_points)
        .ok_or_else(|| anyhow!("net {} driver is not placeable", net.name))?;
    let sinks = net
        .sinks
        .iter()
        .filter_map(|sink| endpoint_grid_point(sink, index, cluster_points, port_points))
        .collect::<Vec<_>>();
    Ok(NetPoints { driver, sinks })
}

fn endpoint_grid_point(
    endpoint: &Endpoint,
    index: &DesignIndex<'_>,
    cluster_points: &[Option<GridPoint>],
    port_points: &[GridPoint],
) -> Option<GridPoint> {
    match index.resolve_endpoint(endpoint) {
        crate::ir::EndpointTarget::Cell(cell_id) => index
            .cluster_for_cell(cell_id)
            .and_then(|cluster_id| cluster_points.get(cluster_id.index()).copied().flatten()),
        crate::ir::EndpointTarget::Port(port_id) => port_points.get(port_id.index()).copied(),
        crate::ir::EndpointTarget::Unknown => None,
    }
}

fn route_points_to_segments(paths: &[Vec<GridPoint>]) -> Vec<RouteSegment> {
    let mut segments = Vec::new();
    for path in paths {
        for window in path.windows(2) {
            segments.push(RouteSegment::new(
                window[0].as_tuple(),
                window[1].as_tuple(),
            ));
        }
    }
    segments
}

#[cfg(test)]
fn points_to_edges(points: &[GridPoint]) -> Vec<EdgeKey> {
    points
        .windows(2)
        .map(|window| canonical_edge(window[0], window[1]))
        .collect()
}

fn segment_edge_usage_dense(nets: &[crate::ir::Net], arch: &Arch) -> EdgeArray<usize> {
    let mut usage = EdgeArray::new(arch.width, arch.height, 0usize);
    for net in nets {
        for segment in &net.route {
            usage.increment(
                GridPoint::new(segment.x0, segment.y0),
                GridPoint::new(segment.x1, segment.y1),
            );
        }
    }
    usage
}

#[cfg(test)]
fn route_congestion_penalty(
    route: &[RouteSegment],
    usage: &BTreeMap<EdgeKey, usize>,
    arch: &Arch,
) -> f64 {
    let usage_dense = EdgeArray::<usize>::from_sparse(arch, usage);
    route_congestion_penalty_dense(route, &usage_dense, arch)
}

fn route_congestion_penalty_dense(
    route: &[RouteSegment],
    usage: &EdgeArray<usize>,
    arch: &Arch,
) -> f64 {
    route
        .iter()
        .map(|segment| {
            let from = GridPoint::new(segment.x0, segment.y0);
            let to = GridPoint::new(segment.x1, segment.y1);
            let edge = canonical_edge(from, to);
            let overflow =
                edge_overflow(usage.get(from, to).copied().unwrap_or(0), edge, arch) as f64;
            if overflow == 0.0 {
                0.0
            } else {
                0.45 * overflow + 0.15 * overflow * overflow
            }
        })
        .sum()
}

fn point_route_delay(route: &[Vec<GridPoint>], arch: &Arch) -> f64 {
    let length = route
        .iter()
        .map(|path| path.len().saturating_sub(1))
        .sum::<usize>() as f64;
    let bends = route.iter().map(|path| path_bends(path)).sum::<usize>() as f64;
    length * (arch.wire_r + arch.wire_c + 0.02) + bends * 0.05
}

fn point_route_congestion_penalty_dense(
    route: &[Vec<GridPoint>],
    usage: &EdgeArray<usize>,
    arch: &Arch,
) -> f64 {
    route
        .iter()
        .flat_map(|path| path.windows(2))
        .map(|window| {
            let edge = canonical_edge(window[0], window[1]);
            let overflow =
                edge_overflow(usage.get(edge.0, edge.1).copied().unwrap_or(0), edge, arch) as f64;
            if overflow == 0.0 {
                0.0
            } else {
                0.45 * overflow + 0.15 * overflow * overflow
            }
        })
        .sum()
}

fn add_route_usage(route: &[Vec<GridPoint>], usage: &mut EdgeArray<usize>) {
    for path in route {
        for window in path.windows(2) {
            usage.increment(window[0], window[1]);
        }
    }
}

fn remove_route_usage(route: &[Vec<GridPoint>], usage: &mut EdgeArray<usize>) {
    for path in route {
        for window in path.windows(2) {
            usage.decrement(window[0], window[1]);
        }
    }
}

fn accumulate_history(
    usage: &EdgeArray<usize>,
    history: &mut EdgeArray<f64>,
    arch: &Arch,
    iteration: usize,
) {
    for (index, count) in usage.horizontal.iter().enumerate() {
        let edge = horizontal_edge(index, arch.width);
        let overflow = edge_overflow(*count, edge, arch);
        if overflow > 0
            && let Some(slot) = history.get_mut(edge.0, edge.1)
        {
            *slot += overflow as f64 * (1.0 + iteration as f64 * 0.15);
        }
    }
    for (index, count) in usage.vertical.iter().enumerate() {
        let edge = vertical_edge(index, arch.width);
        let overflow = edge_overflow(*count, edge, arch);
        if overflow > 0
            && let Some(slot) = history.get_mut(edge.0, edge.1)
        {
            *slot += overflow as f64 * (1.0 + iteration as f64 * 0.15);
        }
    }
}

fn better_metrics(candidate: &RouteMetrics, incumbent: &RouteMetrics, mode: RouteMode) -> bool {
    candidate.overflow < incumbent.overflow
        || (candidate.overflow == incumbent.overflow
            && candidate.overflow_nets < incumbent.overflow_nets)
        || (candidate.overflow == incumbent.overflow
            && candidate.overflow_nets == incumbent.overflow_nets
            && matches!(mode, RouteMode::TimingDriven)
            && candidate.timing_cost + 1e-9 < incumbent.timing_cost)
        || (candidate.overflow == incumbent.overflow
            && candidate.overflow_nets == incumbent.overflow_nets
            && (!matches!(mode, RouteMode::TimingDriven)
                || (candidate.timing_cost - incumbent.timing_cost).abs() <= 1e-9)
            && candidate.total_length < incumbent.total_length)
        || (candidate.overflow == incumbent.overflow
            && candidate.overflow_nets == incumbent.overflow_nets
            && (!matches!(mode, RouteMode::TimingDriven)
                || (candidate.timing_cost - incumbent.timing_cost).abs() <= 1e-9)
            && candidate.total_length == incumbent.total_length
            && candidate.occupied_edges < incumbent.occupied_edges)
}

fn net_overflow_counts(
    point_routes: &[Vec<Vec<GridPoint>>],
    usage: &EdgeArray<usize>,
    arch: &Arch,
) -> Vec<usize> {
    point_routes
        .iter()
        .map(|route| {
            route
                .iter()
                .flat_map(|path| path.windows(2))
                .map(|window| canonical_edge(window[0], window[1]))
                .map(|edge| {
                    edge_overflow(usage.get(edge.0, edge.1).copied().unwrap_or(0), edge, arch)
                })
                .sum()
        })
        .collect()
}

fn farthest_sink_from_tree(
    sinks: &[GridPoint],
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
    arch.edge_capacity(edge.0.into(), edge.1.into())
}

fn canonical_edge(lhs: GridPoint, rhs: GridPoint) -> EdgeKey {
    if lhs <= rhs {
        EdgeKey(lhs, rhs)
    } else {
        EdgeKey(rhs, lhs)
    }
}

fn horizontal_edge(index: usize, width: usize) -> EdgeKey {
    let row_width = width.saturating_sub(1).max(1);
    let y = index / row_width;
    let x = index % row_width;
    canonical_edge(GridPoint::new(x, y), GridPoint::new(x + 1, y))
}

fn vertical_edge(index: usize, width: usize) -> EdgeKey {
    let y = index / width.max(1);
    let x = index % width.max(1);
    canonical_edge(GridPoint::new(x, y), GridPoint::new(x, y + 1))
}

#[cfg(test)]
fn manhattan(lhs: GridPoint, rhs: GridPoint) -> usize {
    lhs.x.abs_diff(rhs.x) + lhs.y.abs_diff(rhs.y)
}

fn grid_index(point: GridPoint, arch: &Arch) -> usize {
    point.y.saturating_mul(arch.width).saturating_add(point.x)
}

fn path_bends(path: &[GridPoint]) -> usize {
    let mut bends = 0usize;
    let mut previous_axis = None;
    for window in path.windows(2) {
        let axis = path_axis(window[0], window[1]);
        if let Some(previous_axis) = previous_axis
            && axis != previous_axis
        {
            bends += 1;
        }
        previous_axis = Some(axis);
    }
    bends
}

fn path_axis(lhs: GridPoint, rhs: GridPoint) -> bool {
    lhs.x != rhs.x
}
