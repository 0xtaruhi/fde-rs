pub use crate::resource::routing::RouteBit;
pub(crate) use crate::resource::routing::{
    RouteNode, SiteRouteArc, SiteRouteGraph, SiteRouteGraphs, WireId, WireInterner,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceRouteImage {
    #[serde(default)]
    pub pips: Vec<DeviceRoutePip>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutedNetPip {
    pub net_name: String,
    pub x: usize,
    pub y: usize,
    pub from_net: String,
    pub to_net: String,
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

#[derive(Debug, Clone, Copy)]
pub(crate) struct RoutedPip {
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) from: WireId,
    pub(crate) to: WireId,
    pub(crate) local_arc: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SearchState<Node, Key> {
    pub(crate) cost: usize,
    pub(crate) priority: usize,
    pub(crate) order: usize,
    pub(crate) key: Key,
    pub(crate) node: Node,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SearchParentStep<Node> {
    pub(crate) previous: Option<Node>,
    pub(crate) local_arc: Option<usize>,
}

impl<Node: Copy + Ord, Key: Copy + Ord> Eq for SearchState<Node, Key> {}

impl<Node: Copy + Ord, Key: Copy + Ord> PartialEq for SearchState<Node, Key> {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
            && self.cost == other.cost
            && self.order == other.order
            && self.key == other.key
            && self.node == other.node
    }
}

impl<Node: Copy + Ord, Key: Copy + Ord> Ord for SearchState<Node, Key> {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| other.cost.cmp(&self.cost))
            .then_with(|| other.order.cmp(&self.order))
            .then_with(|| self.key.cmp(&other.key))
    }
}

impl<Node: Copy + Ord, Key: Copy + Ord> PartialOrd for SearchState<Node, Key> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
