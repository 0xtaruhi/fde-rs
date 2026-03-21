use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::BTreeMap};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceRouteImage {
    #[serde(default)]
    pub pips: Vec<DeviceRoutePip>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceRoutePip {
    pub net_name: String,
    pub tile_name: String,
    pub tile_type: String,
    pub site_name: String,
    pub site_type: String,
    pub x: usize,
    pub y: usize,
    pub from_net: String,
    pub to_net: String,
    #[serde(default)]
    pub bits: Vec<RouteBit>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RouteBit {
    pub basic_cell: String,
    pub sram_name: String,
    pub value: u8,
}

#[derive(Debug, Clone)]
pub(crate) struct SiteRouteGraph {
    pub(crate) arcs: Vec<SiteRouteArc>,
    pub(crate) adjacency: BTreeMap<String, Vec<usize>>,
}

#[derive(Debug, Clone)]
pub(crate) struct SiteRouteArc {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) basic_cell: String,
    pub(crate) bits: Vec<RouteBit>,
}

#[derive(Debug, Clone)]
pub(crate) struct TileRouteSite {
    pub(crate) site_name: String,
    pub(crate) site_type: String,
}

#[derive(Debug, Clone)]
pub(crate) struct GlobalState {
    pub(crate) cost: usize,
    pub(crate) priority: usize,
    pub(crate) node: RouteNode,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RouteNode {
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) net: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ParentStep {
    pub(crate) previous: RouteNode,
    pub(crate) local_arc: Option<usize>,
}

impl Eq for GlobalState {}

impl PartialEq for GlobalState {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.cost == other.cost && self.node == other.node
    }
}

impl Ord for GlobalState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| other.cost.cmp(&self.cost))
            .then_with(|| self.node.net.cmp(&other.node.net))
            .then_with(|| self.node.x.cmp(&other.node.x))
            .then_with(|| self.node.y.cmp(&other.node.y))
    }
}

impl PartialOrd for GlobalState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
