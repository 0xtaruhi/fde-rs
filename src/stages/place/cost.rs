use crate::{
    place::{PlaceMode, manhattan},
    resource::{Arch, DelayModel},
};
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

use super::{
    graph::ClusterGraph,
    model::{PlacementModel, PreparedNet},
};

const CONGESTION_THRESHOLD: f64 = 1.35;
const CONGESTION_SCALE: f64 = 2.5;
const PARALLEL_NET_THRESHOLD: usize = 256;

#[derive(Debug, Clone, Default)]
pub(crate) struct PlacementMetrics {
    pub(crate) wire_cost: f64,
    pub(crate) congestion_cost: f64,
    pub(crate) timing_cost: f64,
    pub(crate) locality_cost: f64,
    pub(crate) total: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct PlacementEvaluator<'a> {
    model: &'a PlacementModel,
    graph: &'a ClusterGraph,
    placements: BTreeMap<String, (usize, usize)>,
    arch: &'a Arch,
    delay: Option<&'a DelayModel>,
    mode: PlaceMode,
    net_models: Vec<Option<NetModel>>,
    loads: Vec<f64>,
    locality_terms: BTreeMap<String, f64>,
    locality_weights: BTreeMap<String, f64>,
    congestion_score_raw: f64,
    metrics: PlacementMetrics,
}

#[derive(Debug, Clone)]
pub(crate) struct PlacementCandidate {
    updates: Vec<(String, (usize, usize))>,
    net_updates: Vec<(usize, Option<NetModel>)>,
    load_deltas: Vec<(usize, f64)>,
    locality_updates: Vec<(String, f64)>,
    metrics: PlacementMetrics,
}

#[derive(Debug, Clone)]
struct NetModel {
    min_x: usize,
    max_x: usize,
    min_y: usize,
    max_y: usize,
    area: usize,
    hpwl: f64,
    route_delay: f64,
    driver_span: f64,
    weight: f64,
}

impl<'a> PlacementEvaluator<'a> {
    pub(crate) fn new(
        model: &'a PlacementModel,
        graph: &'a ClusterGraph,
        placements: BTreeMap<String, (usize, usize)>,
        arch: &'a Arch,
        delay: Option<&'a DelayModel>,
        mode: PlaceMode,
    ) -> Self {
        let net_models = build_net_models(model, &placements, delay, mode);
        let mut wire_cost = 0.0;
        let mut timing_cost = 0.0;
        let mut loads = vec![0.0; arch.width.saturating_mul(arch.height).max(1)];

        for net_model in net_models.iter().flatten() {
            wire_cost += net_model.wire_cost();
            timing_cost += net_model.timing_cost();
            apply_net_load(net_model, arch, 1.0, &mut loads);
        }

        let congestion_score_raw = loads.iter().copied().map(overflow_score).sum::<f64>();
        let locality_weights = graph
            .iter()
            .map(|(cluster, neighbors)| (cluster.clone(), neighbors.values().sum::<f64>()))
            .collect::<BTreeMap<_, _>>();
        let locality_terms = graph
            .keys()
            .filter_map(|cluster| {
                let term = locality_term(
                    cluster,
                    graph,
                    &placements,
                    &BTreeMap::new(),
                    &locality_weights,
                )?;
                Some((cluster.clone(), term))
            })
            .collect::<BTreeMap<_, _>>();
        let locality_cost = locality_terms.values().sum::<f64>();
        let metrics = compose_metrics(
            mode,
            wire_cost,
            congestion_score_raw,
            timing_cost,
            locality_cost,
        );

        Self {
            model,
            graph,
            placements,
            arch,
            delay,
            mode,
            net_models,
            loads,
            locality_terms,
            locality_weights,
            congestion_score_raw,
            metrics,
        }
    }

    pub(crate) fn placements(&self) -> &BTreeMap<String, (usize, usize)> {
        &self.placements
    }

    pub(crate) fn metrics(&self) -> &PlacementMetrics {
        &self.metrics
    }

    pub(crate) fn evaluate_candidate(
        &self,
        updates: &[(String, (usize, usize))],
    ) -> PlacementCandidate {
        if updates.is_empty() {
            return PlacementCandidate {
                updates: Vec::new(),
                net_updates: Vec::new(),
                load_deltas: Vec::new(),
                locality_updates: Vec::new(),
                metrics: self.metrics.clone(),
            };
        }

        let overrides = updates.iter().cloned().collect::<BTreeMap<_, _>>();
        let moved_clusters = updates
            .iter()
            .map(|(cluster, _)| cluster.clone())
            .collect::<Vec<_>>();
        let affected_nets = affected_nets(self.model, &moved_clusters);
        let affected_clusters = affected_locality_clusters(self.graph, &moved_clusters);

        let mut wire_cost = self.metrics.wire_cost;
        let mut timing_cost = self.metrics.timing_cost;
        let mut congestion_score_raw = self.congestion_score_raw;
        let mut locality_cost = self.metrics.locality_cost;
        let mut load_deltas = BTreeMap::<usize, f64>::new();
        let mut net_updates = Vec::with_capacity(affected_nets.len());

        for net_index in affected_nets {
            if let Some(previous) = self.net_models.get(net_index).and_then(Option::as_ref) {
                wire_cost -= previous.wire_cost();
                timing_cost -= previous.timing_cost();
                accumulate_load_delta(previous, self.arch, -1.0, &mut load_deltas);
            }

            let next_model = self.model.nets.get(net_index).and_then(|net| {
                build_net_model_with_overrides(
                    net,
                    self.model,
                    &self.placements,
                    &overrides,
                    self.delay,
                    self.mode,
                )
            });
            if let Some(next) = next_model.as_ref() {
                wire_cost += next.wire_cost();
                timing_cost += next.timing_cost();
                accumulate_load_delta(next, self.arch, 1.0, &mut load_deltas);
            }
            net_updates.push((net_index, next_model));
        }

        for (index, delta) in &load_deltas {
            let previous = self.loads.get(*index).copied().unwrap_or(0.0);
            let next = previous + *delta;
            congestion_score_raw += overflow_score(next) - overflow_score(previous);
        }

        let mut locality_updates = Vec::with_capacity(affected_clusters.len());
        for cluster_name in affected_clusters {
            let previous = self
                .locality_terms
                .get(&cluster_name)
                .copied()
                .unwrap_or(0.0);
            let next = locality_term(
                &cluster_name,
                self.graph,
                &self.placements,
                &overrides,
                &self.locality_weights,
            )
            .unwrap_or(0.0);
            locality_cost += next - previous;
            locality_updates.push((cluster_name, next));
        }

        let metrics = compose_metrics(
            self.mode,
            wire_cost,
            congestion_score_raw,
            timing_cost,
            locality_cost,
        );

        PlacementCandidate {
            updates: updates.to_vec(),
            net_updates,
            load_deltas: load_deltas.into_iter().collect(),
            locality_updates,
            metrics,
        }
    }

    pub(crate) fn apply_candidate(&mut self, candidate: PlacementCandidate) {
        for (cluster, position) in candidate.updates {
            self.placements.insert(cluster, position);
        }

        for (index, delta) in candidate.load_deltas {
            if let Some(load) = self.loads.get_mut(index) {
                *load += delta;
            }
        }

        for (net_index, net_model) in candidate.net_updates {
            if let Some(slot) = self.net_models.get_mut(net_index) {
                *slot = net_model;
            }
        }

        for (cluster_name, locality_term) in candidate.locality_updates {
            self.locality_terms.insert(cluster_name, locality_term);
        }

        self.congestion_score_raw = candidate.metrics.congestion_cost / CONGESTION_SCALE;
        self.metrics = candidate.metrics;
    }
}

impl PlacementCandidate {
    pub(crate) fn metrics(&self) -> &PlacementMetrics {
        &self.metrics
    }
}

pub(crate) fn evaluate(
    model: &PlacementModel,
    graph: &ClusterGraph,
    placements: &BTreeMap<String, (usize, usize)>,
    arch: &Arch,
    delay: Option<&DelayModel>,
    mode: PlaceMode,
) -> PlacementMetrics {
    PlacementEvaluator::new(model, graph, placements.clone(), arch, delay, mode)
        .metrics
        .clone()
}

fn build_net_models(
    model: &PlacementModel,
    placements: &BTreeMap<String, (usize, usize)>,
    delay: Option<&DelayModel>,
    mode: PlaceMode,
) -> Vec<Option<NetModel>> {
    if model.nets.len() >= PARALLEL_NET_THRESHOLD {
        model
            .nets
            .par_iter()
            .map(|net| build_net_model(net, model, placements, delay, mode))
            .collect::<Vec<_>>()
    } else {
        model
            .nets
            .iter()
            .map(|net| build_net_model(net, model, placements, delay, mode))
            .collect::<Vec<_>>()
    }
}

fn affected_nets(model: &PlacementModel, moved_clusters: &[String]) -> Vec<usize> {
    let mut nets = BTreeSet::<usize>::new();
    for cluster_name in moved_clusters {
        for net_index in model.nets_for_cluster(cluster_name) {
            nets.insert(*net_index);
        }
    }
    nets.into_iter().collect()
}

fn affected_locality_clusters(graph: &ClusterGraph, moved_clusters: &[String]) -> Vec<String> {
    let mut affected = BTreeSet::<String>::new();
    for cluster_name in moved_clusters {
        affected.insert(cluster_name.clone());
        if let Some(neighbors) = graph.get(cluster_name) {
            for neighbor in neighbors.keys() {
                affected.insert(neighbor.clone());
            }
        }
    }
    affected.into_iter().collect()
}

fn locality_term(
    cluster_name: &str,
    graph: &ClusterGraph,
    placements: &BTreeMap<String, (usize, usize)>,
    overrides: &BTreeMap<String, (usize, usize)>,
    locality_weights: &BTreeMap<String, f64>,
) -> Option<f64> {
    let position = lookup_position(cluster_name, placements, overrides)?;
    let centroid = weighted_centroid_with_overrides(cluster_name, graph, placements, overrides)?;
    let weight = locality_weights.get(cluster_name).copied().unwrap_or(0.0);
    Some(0.08 * weight * manhattan(position, centroid) as f64)
}

fn weighted_centroid_with_overrides(
    cluster_name: &str,
    graph: &ClusterGraph,
    placements: &BTreeMap<String, (usize, usize)>,
    overrides: &BTreeMap<String, (usize, usize)>,
) -> Option<(usize, usize)> {
    let mut x_total = 0.0;
    let mut y_total = 0.0;
    let mut weight_total = 0.0;

    for (neighbor, weight) in graph.get(cluster_name)? {
        let point = lookup_position(neighbor, placements, overrides)?;
        x_total += point.0 as f64 * weight;
        y_total += point.1 as f64 * weight;
        weight_total += weight;
    }

    if weight_total == 0.0 {
        None
    } else {
        Some((
            (x_total / weight_total).round() as usize,
            (y_total / weight_total).round() as usize,
        ))
    }
}

fn lookup_position(
    cluster_name: &str,
    placements: &BTreeMap<String, (usize, usize)>,
    overrides: &BTreeMap<String, (usize, usize)>,
) -> Option<(usize, usize)> {
    overrides
        .get(cluster_name)
        .copied()
        .or_else(|| placements.get(cluster_name).copied())
}

fn compose_metrics(
    mode: PlaceMode,
    wire_cost: f64,
    congestion_score_raw: f64,
    timing_cost: f64,
    locality_cost: f64,
) -> PlacementMetrics {
    let congestion_cost = congestion_score_raw * CONGESTION_SCALE;
    let total = match mode {
        PlaceMode::BoundingBox => wire_cost + 0.75 * congestion_cost + 0.50 * locality_cost,
        PlaceMode::TimingDriven => {
            wire_cost + 1.15 * congestion_cost + 1.35 * timing_cost + 0.75 * locality_cost
        }
    };

    PlacementMetrics {
        wire_cost,
        congestion_cost,
        timing_cost,
        locality_cost,
        total,
    }
}

fn overflow_score(load: f64) -> f64 {
    let overflow = (load - CONGESTION_THRESHOLD).max(0.0);
    overflow * overflow
}

fn accumulate_load_delta(
    net_model: &NetModel,
    arch: &Arch,
    scale: f64,
    deltas: &mut BTreeMap<usize, f64>,
) {
    let cell_load = net_model.cell_load() * scale;
    for x in net_model.min_x..=net_model.max_x {
        for y in net_model.min_y..=net_model.max_y {
            let index = y * arch.width + x;
            *deltas.entry(index).or_insert(0.0) += cell_load;
        }
    }
}

fn apply_net_load(net_model: &NetModel, arch: &Arch, scale: f64, loads: &mut [f64]) {
    let cell_load = net_model.cell_load() * scale;
    for x in net_model.min_x..=net_model.max_x {
        for y in net_model.min_y..=net_model.max_y {
            let index = y * arch.width + x;
            if let Some(load) = loads.get_mut(index) {
                *load += cell_load;
            }
        }
    }
}

fn build_net_model(
    net: &PreparedNet,
    model: &PlacementModel,
    placements: &BTreeMap<String, (usize, usize)>,
    delay: Option<&DelayModel>,
    mode: PlaceMode,
) -> Option<NetModel> {
    build_net_model_with_overrides(net, model, placements, &BTreeMap::new(), delay, mode)
}

fn build_net_model_with_overrides(
    net: &PreparedNet,
    model: &PlacementModel,
    placements: &BTreeMap<String, (usize, usize)>,
    overrides: &BTreeMap<String, (usize, usize)>,
    delay: Option<&DelayModel>,
    mode: PlaceMode,
) -> Option<NetModel> {
    let driver = net.driver.as_ref()?;
    let src = model.point_for_overrides(driver, placements, overrides)?;
    let mut points = vec![src];
    for sink in &net.sinks {
        if let Some(point) = model.point_for_overrides(sink, placements, overrides) {
            points.push(point);
        }
    }
    if points.len() <= 1 {
        return None;
    }

    let (min_x, max_x) = points.iter().fold((usize::MAX, 0usize), |acc, point| {
        (acc.0.min(point.0), acc.1.max(point.0))
    });
    let (min_y, max_y) = points.iter().fold((usize::MAX, 0usize), |acc, point| {
        (acc.0.min(point.1), acc.1.max(point.1))
    });
    let dx = max_x - min_x;
    let dy = max_y - min_y;
    let hpwl = (dx + dy) as f64;
    let route_delay = delay
        .map(|table| table.lookup(dx, dy))
        .unwrap_or(hpwl * 0.08);
    let fanout = net.fanout as f64;
    let driver_span = net
        .sinks
        .iter()
        .filter_map(|sink| model.point_for_overrides(sink, placements, overrides))
        .map(|sink| manhattan(src, sink) as f64)
        .fold(0.0, f64::max);
    let base_weight = 1.0 + 0.12 * fanout.min(8.0);
    let weight = match mode {
        PlaceMode::BoundingBox => base_weight,
        PlaceMode::TimingDriven => base_weight + 1.4 * net.criticality.max(0.0),
    };

    Some(NetModel {
        min_x,
        max_x,
        min_y,
        max_y,
        area: (dx + 1) * (dy + 1),
        hpwl,
        route_delay,
        driver_span,
        weight,
    })
}

impl NetModel {
    fn cell_load(&self) -> f64 {
        self.weight / self.area.max(1) as f64
    }

    fn wire_cost(&self) -> f64 {
        let span_area = (self.max_x - self.min_x + 1) * (self.max_y - self.min_y + 1);
        self.weight * (self.hpwl + 0.35 * self.route_delay + 0.08 * (span_area as f64).sqrt())
    }

    fn timing_cost(&self) -> f64 {
        self.weight * (self.route_delay + 0.12 * self.driver_span)
    }
}
