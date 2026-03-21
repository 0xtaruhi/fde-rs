use crate::ir::Endpoint;
use std::collections::BTreeMap;

pub(crate) type ArrivalMap = BTreeMap<String, f64>;

pub(crate) fn endpoint_arrival_key(endpoint: &Endpoint) -> String {
    endpoint.key()
}

pub(crate) fn port_arrival_key(port_name: &str) -> String {
    format!("port:{port_name}")
}

pub(crate) fn cell_arrival_key(cell_name: &str, pin_name: &str) -> String {
    format!("cell:{cell_name}:{pin_name}")
}

pub(crate) fn net_arrival_key(net_name: &str) -> String {
    format!("net:{net_name}")
}
