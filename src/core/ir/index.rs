use std::collections::HashMap;

use crate::domain::EndpointKind;

use super::{
    Cell, CellId, Cluster, ClusterId, Design, Endpoint, EndpointKey, Net, NetId, Port, PortId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EndpointTarget {
    Cell(CellId),
    Port(PortId),
    Unknown,
}

impl EndpointTarget {
    pub fn cell_id(self) -> Option<CellId> {
        match self {
            Self::Cell(cell_id) => Some(cell_id),
            Self::Port(_) | Self::Unknown => None,
        }
    }

    pub fn port_id(self) -> Option<PortId> {
        match self {
            Self::Port(port_id) => Some(port_id),
            Self::Cell(_) | Self::Unknown => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DesignIndex<'a> {
    ports_by_name: HashMap<&'a str, PortId>,
    cells_by_name: HashMap<&'a str, CellId>,
    nets_by_name: HashMap<&'a str, NetId>,
    cluster_of_cell: Vec<Option<ClusterId>>,
    cluster_members: Vec<Vec<CellId>>,
    sink_to_net: HashMap<EndpointKey, NetId>,
}

impl<'a> DesignIndex<'a> {
    pub fn build(design: &'a Design) -> Self {
        let ports_by_name = design
            .ports
            .iter()
            .enumerate()
            .map(|(index, port)| (port.name.as_str(), PortId::new(index)))
            .collect();
        let cells_by_name = design
            .cells
            .iter()
            .enumerate()
            .map(|(index, cell)| (cell.name.as_str(), CellId::new(index)))
            .collect::<HashMap<_, _>>();
        let nets_by_name = design
            .nets
            .iter()
            .enumerate()
            .map(|(index, net)| (net.name.as_str(), NetId::new(index)))
            .collect();
        let clusters_by_name = design
            .clusters
            .iter()
            .enumerate()
            .map(|(index, cluster)| (cluster.name.as_str(), ClusterId::new(index)))
            .collect::<HashMap<_, _>>();

        let cluster_of_cell = design
            .cells
            .iter()
            .map(|cell| {
                cell.cluster
                    .as_ref()
                    .and_then(|name| clusters_by_name.get(name.as_str()).copied())
            })
            .collect::<Vec<_>>();

        let cluster_members = design
            .clusters
            .iter()
            .map(|cluster| {
                cluster
                    .members
                    .iter()
                    .filter_map(|member| cells_by_name.get(member.as_str()).copied())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let sink_to_net = design
            .nets
            .iter()
            .enumerate()
            .flat_map(|(index, net)| {
                net.sinks
                    .iter()
                    .map(move |sink| (sink.key(), NetId::new(index)))
            })
            .collect();

        Self {
            ports_by_name,
            cells_by_name,
            nets_by_name,
            cluster_of_cell,
            cluster_members,
            sink_to_net,
        }
    }

    pub fn port_id(&self, name: &str) -> Option<PortId> {
        self.ports_by_name.get(name).copied()
    }

    pub fn cell_id(&self, name: &str) -> Option<CellId> {
        self.cells_by_name.get(name).copied()
    }

    pub fn net_id(&self, name: &str) -> Option<NetId> {
        self.nets_by_name.get(name).copied()
    }

    pub fn port<'design>(&self, design: &'design Design, id: PortId) -> &'design Port {
        &design.ports[id.index()]
    }

    pub fn cell<'design>(&self, design: &'design Design, id: CellId) -> &'design Cell {
        &design.cells[id.index()]
    }

    pub fn net<'design>(&self, design: &'design Design, id: NetId) -> &'design Net {
        &design.nets[id.index()]
    }

    pub fn cluster<'design>(&self, design: &'design Design, id: ClusterId) -> &'design Cluster {
        &design.clusters[id.index()]
    }

    pub fn cluster_for_cell(&self, cell_id: CellId) -> Option<ClusterId> {
        self.cluster_of_cell.get(cell_id.index()).copied().flatten()
    }

    pub fn cluster_members(&self, cluster_id: ClusterId) -> &[CellId] {
        self.cluster_members
            .get(cluster_id.index())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn resolve_endpoint(&self, endpoint: &Endpoint) -> EndpointTarget {
        match endpoint.kind {
            EndpointKind::Cell => self
                .cell_id(&endpoint.name)
                .map(EndpointTarget::Cell)
                .unwrap_or(EndpointTarget::Unknown),
            EndpointKind::Port => self
                .port_id(&endpoint.name)
                .map(EndpointTarget::Port)
                .unwrap_or(EndpointTarget::Unknown),
            EndpointKind::Unknown => EndpointTarget::Unknown,
        }
    }

    pub fn cell_for_endpoint(&self, endpoint: &Endpoint) -> Option<CellId> {
        self.resolve_endpoint(endpoint).cell_id()
    }

    pub fn port_for_endpoint(&self, endpoint: &Endpoint) -> Option<PortId> {
        self.resolve_endpoint(endpoint).port_id()
    }

    pub fn cluster_for_endpoint(&self, endpoint: &Endpoint) -> Option<ClusterId> {
        self.cell_for_endpoint(endpoint)
            .and_then(|cell_id| self.cluster_for_cell(cell_id))
    }

    pub fn net_for_sink(&self, endpoint: &Endpoint) -> Option<NetId> {
        self.sink_to_net.get(&endpoint.key()).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::EndpointTarget;
    use crate::ir::{Cell, Cluster, Design, Endpoint, Net, Port};

    #[test]
    fn resolves_endpoints_to_typed_targets() {
        let design = Design {
            ports: vec![Port::input("in")],
            cells: vec![Cell::lut("u0", "LUT4").in_cluster("clb0")],
            nets: vec![
                Net::new("n0")
                    .with_driver(Endpoint::port("in", "IN"))
                    .with_sink(Endpoint::cell("u0", "A")),
            ],
            clusters: vec![Cluster::logic("clb0").with_member("u0").with_capacity(1)],
            ..Design::default()
        };
        let index = design.index();

        let port_endpoint = Endpoint::port("in", "IN");
        let cell_endpoint = Endpoint::cell("u0", "A");
        let unknown_endpoint = Endpoint::cell("ghost", "Q");

        assert!(matches!(
            index.resolve_endpoint(&port_endpoint),
            EndpointTarget::Port(_)
        ));
        assert!(matches!(
            index.resolve_endpoint(&cell_endpoint),
            EndpointTarget::Cell(_)
        ));
        assert_eq!(
            index.resolve_endpoint(&unknown_endpoint),
            EndpointTarget::Unknown
        );
        assert!(index.cluster_for_endpoint(&cell_endpoint).is_some());
        assert!(index.cluster_for_endpoint(&port_endpoint).is_none());
        let cluster_id = index
            .cluster_for_endpoint(&cell_endpoint)
            .expect("clb0 cluster");
        assert_eq!(index.cluster_members(cluster_id).len(), 1);
        assert_eq!(
            index.cluster_members(cluster_id)[0],
            index.cell_id("u0").expect("u0 cell")
        );
    }
}
