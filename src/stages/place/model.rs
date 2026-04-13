use crate::ir::{ClusterId, Design, DesignIndex, Endpoint};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub(crate) struct Point {
    pub(crate) x: usize,
    pub(crate) y: usize,
}

impl Point {
    pub(crate) const fn new(x: usize, y: usize) -> Self {
        Self { x, y }
    }

    pub(crate) const fn as_tuple(self) -> (usize, usize) {
        (self.x, self.y)
    }
}

impl From<(usize, usize)> for Point {
    fn from(value: (usize, usize)) -> Self {
        Self::new(value.0, value.1)
    }
}

impl From<Point> for (usize, usize) {
    fn from(value: Point) -> Self {
        value.as_tuple()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PlacementEndpoint {
    Cluster(ClusterId),
    Port(Point),
}

impl PlacementEndpoint {
    fn cluster_id(self) -> Option<ClusterId> {
        match self {
            Self::Cluster(cluster_id) => Some(cluster_id),
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
    fixed_clusters: Vec<Option<Point>>,
    nets_by_cluster: Vec<Vec<usize>>,
}

impl PlacementModel {
    pub(crate) fn from_design(design: &Design) -> Self {
        let index = design.index();
        let fixed_clusters = design
            .clusters
            .iter()
            .map(|cluster| cluster.x.zip(cluster.y).map(Point::from))
            .collect::<Vec<_>>();
        let port_points = design
            .ports
            .iter()
            .map(|port| port.x.zip(port.y).map(Point::from))
            .collect::<Vec<_>>();

        let mut nets = Vec::with_capacity(design.nets.len());
        let mut nets_by_cluster = vec![Vec::new(); design.clusters.len()];
        for net in &design.nets {
            let driver = net
                .driver
                .as_ref()
                .and_then(|endpoint| resolve_endpoint(endpoint, &index, &port_points));
            let sinks = net
                .sinks
                .iter()
                .filter_map(|endpoint| resolve_endpoint(endpoint, &index, &port_points))
                .collect::<Vec<_>>();
            let net_index = nets.len();
            let mut touched = vec![false; design.clusters.len()];
            if let Some(cluster_id) = driver.and_then(PlacementEndpoint::cluster_id) {
                touched[cluster_id.index()] = true;
            }
            for sink in &sinks {
                if let Some(cluster_id) = sink.cluster_id() {
                    touched[cluster_id.index()] = true;
                }
            }
            for (cluster_index, include) in touched.into_iter().enumerate() {
                if include {
                    nets_by_cluster[cluster_index].push(net_index);
                }
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

    pub(crate) fn cluster_count(&self) -> usize {
        self.fixed_clusters.len()
    }

    pub(crate) fn fixed_placements(&self) -> Vec<Option<Point>> {
        self.fixed_clusters.clone()
    }

    pub(crate) fn fixed_point(&self, cluster_id: ClusterId) -> Option<Point> {
        self.fixed_clusters
            .get(cluster_id.index())
            .copied()
            .flatten()
    }

    pub(crate) fn point_for_overrides(
        &self,
        endpoint: PlacementEndpoint,
        placements: &[Option<Point>],
        overrides: &[(ClusterId, Point)],
    ) -> Option<Point> {
        match endpoint {
            PlacementEndpoint::Cluster(cluster_id) => overrides
                .iter()
                .rev()
                .find(|(candidate, _)| *candidate == cluster_id)
                .map(|(_, point)| *point)
                .or_else(|| placements.get(cluster_id.index()).copied().flatten())
                .or_else(|| self.fixed_point(cluster_id)),
            PlacementEndpoint::Port(point) => Some(point),
        }
    }

    pub(crate) fn nets_for_cluster(&self, cluster_id: ClusterId) -> &[usize] {
        self.nets_by_cluster
            .get(cluster_id.index())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(crate) fn signal_centroid(
        &self,
        cluster_id: ClusterId,
        placements: &[Option<Point>],
    ) -> Option<Point> {
        self.signal_centroid_with_overrides(cluster_id, placements, &[])
    }

    pub(crate) fn signal_centroid_with_overrides(
        &self,
        cluster_id: ClusterId,
        placements: &[Option<Point>],
        overrides: &[(ClusterId, Point)],
    ) -> Option<Point> {
        let mut x_total = 0.0;
        let mut y_total = 0.0;
        let mut weight_total = 0.0;

        for net_index in self.nets_for_cluster(cluster_id) {
            let net = self.nets.get(*net_index)?;
            let weight = 1.0 + net.criticality.max(0.0);
            let mut points = Vec::with_capacity(net.sinks.len() + 1);
            if let Some(driver) = net.driver
                && let Some(point) = self.point_for_overrides(driver, placements, overrides)
            {
                points.push(point);
            }
            for sink in &net.sinks {
                if let Some(point) = self.point_for_overrides(*sink, placements, overrides) {
                    points.push(point);
                }
            }
            if points.is_empty() {
                continue;
            }

            let center_x =
                points.iter().map(|point| point.x as f64).sum::<f64>() / points.len() as f64;
            let center_y =
                points.iter().map(|point| point.y as f64).sum::<f64>() / points.len() as f64;
            x_total += center_x * weight;
            y_total += center_y * weight;
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

fn resolve_endpoint(
    endpoint: &Endpoint,
    index: &DesignIndex<'_>,
    port_points: &[Option<Point>],
) -> Option<PlacementEndpoint> {
    match index.resolve_endpoint(endpoint) {
        crate::ir::EndpointTarget::Cell(cell_id) => index
            .cluster_for_cell(cell_id)
            .map(PlacementEndpoint::Cluster),
        crate::ir::EndpointTarget::Port(port_id) => port_points
            .get(port_id.index())
            .copied()
            .flatten()
            .map(PlacementEndpoint::Port),
        crate::ir::EndpointTarget::Unknown => None,
    }
}
