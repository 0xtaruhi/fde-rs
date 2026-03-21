use crate::ir::{Design, Endpoint};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub(crate) enum PlacementEndpoint {
    Cluster(String),
    Port((usize, usize)),
}

impl PlacementEndpoint {
    fn cluster_name(&self) -> Option<&str> {
        match self {
            Self::Cluster(name) => Some(name.as_str()),
            Self::Port(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedNet {
    pub(crate) driver: Option<PlacementEndpoint>,
    pub(crate) sinks: Vec<PlacementEndpoint>,
    pub(crate) criticality: f64,
    pub(crate) fanout: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PlacementModel {
    pub(crate) nets: Vec<PreparedNet>,
    fixed_clusters: BTreeMap<String, (usize, usize)>,
    nets_by_cluster: BTreeMap<String, Vec<usize>>,
}

impl PlacementModel {
    pub(crate) fn from_design(design: &Design) -> Self {
        let cluster_by_cell = design
            .cells
            .iter()
            .filter_map(|cell| {
                cell.cluster
                    .as_ref()
                    .map(|cluster| (cell.name.clone(), cluster.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let fixed_clusters = design
            .clusters
            .iter()
            .filter_map(|cluster| Some((cluster.name.clone(), (cluster.x?, cluster.y?))))
            .collect::<BTreeMap<_, _>>();
        let port_points = design
            .ports
            .iter()
            .filter_map(|port| Some((port.name.clone(), (port.x?, port.y?))))
            .collect::<BTreeMap<_, _>>();

        let mut nets = Vec::with_capacity(design.nets.len());
        let mut nets_by_cluster = BTreeMap::<String, Vec<usize>>::new();
        for net in &design.nets {
            let driver = net
                .driver
                .as_ref()
                .and_then(|endpoint| resolve_endpoint(endpoint, &cluster_by_cell, &port_points));
            let sinks = net
                .sinks
                .iter()
                .filter_map(|endpoint| resolve_endpoint(endpoint, &cluster_by_cell, &port_points))
                .collect::<Vec<_>>();
            let net_index = nets.len();
            let touched_clusters = driver
                .iter()
                .chain(sinks.iter())
                .filter_map(PlacementEndpoint::cluster_name)
                .map(ToOwned::to_owned)
                .collect::<BTreeSet<_>>();
            for cluster_name in touched_clusters {
                nets_by_cluster
                    .entry(cluster_name)
                    .or_default()
                    .push(net_index);
            }

            nets.push(PreparedNet {
                driver,
                sinks,
                criticality: net.criticality,
                fanout: net.sinks.len(),
            });
        }

        Self {
            nets,
            fixed_clusters,
            nets_by_cluster,
        }
    }

    pub(crate) fn point_for_overrides(
        &self,
        endpoint: &PlacementEndpoint,
        placements: &BTreeMap<String, (usize, usize)>,
        overrides: &BTreeMap<String, (usize, usize)>,
    ) -> Option<(usize, usize)> {
        match endpoint {
            PlacementEndpoint::Cluster(cluster) => overrides
                .get(cluster)
                .copied()
                .or_else(|| placements.get(cluster).copied())
                .or_else(|| self.fixed_clusters.get(cluster).copied()),
            PlacementEndpoint::Port(point) => Some(*point),
        }
    }

    pub(crate) fn nets_for_cluster(&self, cluster_name: &str) -> &[usize] {
        self.nets_by_cluster
            .get(cluster_name)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(crate) fn signal_centroid(
        &self,
        cluster_name: &str,
        placements: &BTreeMap<String, (usize, usize)>,
    ) -> Option<(usize, usize)> {
        self.signal_centroid_with_overrides(cluster_name, placements, &BTreeMap::new())
    }

    pub(crate) fn signal_centroid_with_overrides(
        &self,
        cluster_name: &str,
        placements: &BTreeMap<String, (usize, usize)>,
        overrides: &BTreeMap<String, (usize, usize)>,
    ) -> Option<(usize, usize)> {
        let mut x_total = 0.0;
        let mut y_total = 0.0;
        let mut weight_total = 0.0;

        for net_index in self.nets_by_cluster.get(cluster_name)? {
            let net = self.nets.get(*net_index)?;
            let weight = 1.0 + net.criticality.max(0.0);
            let mut points = Vec::with_capacity(net.sinks.len() + 1);
            if let Some(driver) = &net.driver
                && let Some(point) = self.point_for_overrides(driver, placements, overrides)
            {
                points.push(point);
            }
            for sink in &net.sinks {
                if let Some(point) = self.point_for_overrides(sink, placements, overrides) {
                    points.push(point);
                }
            }
            if points.is_empty() {
                continue;
            }

            let center_x =
                points.iter().map(|point| point.0 as f64).sum::<f64>() / points.len() as f64;
            let center_y =
                points.iter().map(|point| point.1 as f64).sum::<f64>() / points.len() as f64;
            x_total += center_x * weight;
            y_total += center_y * weight;
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
}

fn resolve_endpoint(
    endpoint: &Endpoint,
    cluster_by_cell: &BTreeMap<String, String>,
    port_points: &BTreeMap<String, (usize, usize)>,
) -> Option<PlacementEndpoint> {
    match endpoint.endpoint_kind() {
        crate::domain::EndpointKind::Cell => cluster_by_cell
            .get(&endpoint.name)
            .cloned()
            .map(PlacementEndpoint::Cluster),
        crate::domain::EndpointKind::Port => port_points
            .get(&endpoint.name)
            .copied()
            .map(PlacementEndpoint::Port),
        crate::domain::EndpointKind::Unknown => None,
    }
}
