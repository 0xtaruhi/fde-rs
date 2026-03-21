use crate::ir::{ClusterId, Design};
use std::collections::HashMap;

use super::model::Point;

#[derive(Debug, Clone, Default)]
pub(crate) struct ClusterGraph {
    adjacency: Vec<Vec<(ClusterId, f64)>>,
    totals: Vec<f64>,
}

impl ClusterGraph {
    pub(crate) fn neighbors(&self, cluster_id: ClusterId) -> &[(ClusterId, f64)] {
        self.adjacency
            .get(cluster_id.index())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(crate) fn total_weight(&self, cluster_id: ClusterId) -> f64 {
        self.totals.get(cluster_id.index()).copied().unwrap_or(0.0)
    }

    pub(crate) fn weighted_centroid(
        &self,
        cluster_id: ClusterId,
        placements: &[Option<Point>],
    ) -> Option<Point> {
        let mut x_total = 0.0;
        let mut y_total = 0.0;
        let mut weight_total = 0.0;

        for (neighbor, weight) in self.neighbors(cluster_id) {
            let Some(point) = placements.get(neighbor.index()).copied().flatten() else {
                continue;
            };
            x_total += point.x as f64 * weight;
            y_total += point.y as f64 * weight;
            weight_total += weight;
        }

        if weight_total == 0.0 {
            None
        } else {
            Some(Point::new(
                (x_total / weight_total).round() as usize,
                (y_total / weight_total).round() as usize,
            ))
        }
    }
}

pub(crate) fn build_cluster_graph(design: &Design) -> ClusterGraph {
    let index = design.index();
    let cluster_count = design.clusters.len();
    let mut adjacency = (0..cluster_count)
        .map(|_| HashMap::<ClusterId, f64>::new())
        .collect::<Vec<_>>();
    let mut totals = vec![0.0; cluster_count];

    for net in &design.nets {
        let Some(driver) = &net.driver else {
            continue;
        };
        let Some(src_cluster) = index.cluster_for_endpoint(driver) else {
            continue;
        };

        let fanout = net.sinks.len().max(1) as f64;
        for sink in &net.sinks {
            let Some(dst_cluster) = index.cluster_for_endpoint(sink) else {
                continue;
            };
            if src_cluster == dst_cluster {
                continue;
            }
            let weight = 1.0 / fanout;
            add_neighbor_weight(
                &mut adjacency,
                &mut totals,
                src_cluster,
                dst_cluster,
                weight,
            );
            add_neighbor_weight(
                &mut adjacency,
                &mut totals,
                dst_cluster,
                src_cluster,
                weight,
            );
        }
    }

    let mut graph = ClusterGraph {
        adjacency: vec![Vec::new(); cluster_count],
        totals,
    };
    for (cluster_index, neighbors) in adjacency.into_iter().enumerate() {
        let mut ordered = neighbors.into_iter().collect::<Vec<_>>();
        ordered.sort_by(|lhs, rhs| lhs.0.cmp(&rhs.0));
        graph.adjacency[cluster_index] = ordered;
    }

    graph
}

fn add_neighbor_weight(
    adjacency: &mut [HashMap<ClusterId, f64>],
    totals: &mut [f64],
    src: ClusterId,
    dst: ClusterId,
    weight: f64,
) {
    *adjacency[src.index()].entry(dst).or_insert(0.0) += weight;
    totals[src.index()] += weight;
}

pub(crate) fn cluster_incident_criticality(design: &Design) -> Vec<f64> {
    let index = design.index();
    let mut totals = vec![0.0; design.clusters.len()];
    for net in &design.nets {
        let weight = 1.0 + net.criticality.max(0.0);
        if let Some(driver) = &net.driver
            && let Some(cluster_id) = index.cluster_for_endpoint(driver)
        {
            totals[cluster_id.index()] += weight;
        }
        for sink in &net.sinks {
            if let Some(cluster_id) = index.cluster_for_endpoint(sink) {
                totals[cluster_id.index()] += weight;
            }
        }
    }
    totals
}
