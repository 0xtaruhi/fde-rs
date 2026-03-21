use crate::ir::{
    CellId, Design, DesignIndex, Endpoint, EndpointKey, EndpointTarget, NetId, PortId,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum TimingEndpoint {
    Port { port_id: PortId, pin: String },
    Cell { cell_id: CellId, pin: String },
    Unknown(EndpointKey),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum TimingKey {
    Port(PortId),
    Endpoint(TimingEndpoint),
    Net(NetId),
}

pub(crate) type ArrivalMap = BTreeMap<TimingKey, f64>;

impl TimingEndpoint {
    pub(crate) fn from_endpoint(index: &DesignIndex<'_>, endpoint: &Endpoint) -> Self {
        match index.resolve_endpoint(endpoint) {
            EndpointTarget::Port(port_id) => Self::Port {
                port_id,
                pin: endpoint.pin.clone(),
            },
            EndpointTarget::Cell(cell_id) => Self::Cell {
                cell_id,
                pin: endpoint.pin.clone(),
            },
            EndpointTarget::Unknown => Self::Unknown(endpoint.key()),
        }
    }
}

pub(crate) fn endpoint_arrival_key(index: &DesignIndex<'_>, endpoint: &Endpoint) -> TimingKey {
    TimingKey::Endpoint(TimingEndpoint::from_endpoint(index, endpoint))
}

pub(crate) fn port_arrival_key(port_id: PortId) -> TimingKey {
    TimingKey::Port(port_id)
}

pub(crate) fn cell_arrival_key(cell_id: CellId, pin_name: &str) -> TimingKey {
    TimingKey::Endpoint(TimingEndpoint::Cell {
        cell_id,
        pin: pin_name.to_string(),
    })
}

pub(crate) fn net_arrival_key(net_id: NetId) -> TimingKey {
    TimingKey::Net(net_id)
}

pub(crate) fn render_timing_key(
    design: &Design,
    index: &DesignIndex<'_>,
    key: &TimingKey,
) -> String {
    match key {
        TimingKey::Port(port_id) => {
            let port = index.port(design, *port_id);
            format!("port:{}", port.name)
        }
        TimingKey::Endpoint(endpoint) => render_timing_endpoint_key(design, index, endpoint),
        TimingKey::Net(net_id) => {
            let net = index.net(design, *net_id);
            format!("net:{}", net.name)
        }
    }
}

pub(crate) fn render_endpoint_label(
    design: &Design,
    index: &DesignIndex<'_>,
    endpoint: &TimingEndpoint,
) -> String {
    match endpoint {
        TimingEndpoint::Port { port_id, pin } => {
            let port = index.port(design, *port_id);
            format!("{}:{pin}", port.name)
        }
        TimingEndpoint::Cell { cell_id, pin } => {
            let cell = index.cell(design, *cell_id);
            format!("{}:{pin}", cell.name)
        }
        TimingEndpoint::Unknown(endpoint) => format!("{}:{}", endpoint.name(), endpoint.pin()),
    }
}

fn render_timing_endpoint_key(
    design: &Design,
    index: &DesignIndex<'_>,
    endpoint: &TimingEndpoint,
) -> String {
    match endpoint {
        TimingEndpoint::Port { port_id, pin } => {
            let port = index.port(design, *port_id);
            format!("port:{}:{pin}", port.name)
        }
        TimingEndpoint::Cell { cell_id, pin } => {
            let cell = index.cell(design, *cell_id);
            format!("cell:{}:{pin}", cell.name)
        }
        TimingEndpoint::Unknown(endpoint) => endpoint.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TimingEndpoint, TimingKey, cell_arrival_key, endpoint_arrival_key, net_arrival_key,
        port_arrival_key, render_endpoint_label, render_timing_key,
    };
    use crate::ir::{Cell, Cluster, Design, Endpoint, Net, Port};

    fn mini_design() -> Design {
        Design {
            ports: vec![Port::input("in"), Port::output("out")],
            cells: vec![Cell::lut("u0", "LUT4").in_cluster("clb0")],
            nets: vec![
                Net::new("n0")
                    .with_driver(Endpoint::port("in", "IN"))
                    .with_sink(Endpoint::cell("u0", "A")),
            ],
            clusters: vec![Cluster::logic("clb0").with_member("u0").with_capacity(1)],
            ..Design::default()
        }
    }

    #[test]
    fn preserves_existing_timing_key_strings() {
        let design = mini_design();
        let index = design.index();
        let port_id = index.port_id("in").expect("in port");
        let net_id = index.net_id("n0").expect("n0 net");
        let cell_id = index.cell_id("u0").expect("u0 cell");

        assert_eq!(
            render_timing_key(&design, &index, &port_arrival_key(port_id)),
            "port:in"
        );
        assert_eq!(
            render_timing_key(&design, &index, &cell_arrival_key(cell_id, "O")),
            "cell:u0:O"
        );
        assert_eq!(
            render_timing_key(&design, &index, &net_arrival_key(net_id)),
            "net:n0"
        );

        let endpoint_key = endpoint_arrival_key(&index, &Endpoint::cell("u0", "Q"));
        assert!(matches!(
            endpoint_key,
            TimingKey::Endpoint(TimingEndpoint::Cell { .. })
        ));
        if let TimingKey::Endpoint(endpoint) = endpoint_key {
            assert_eq!(render_endpoint_label(&design, &index, &endpoint), "u0:Q");
        }
    }
}
