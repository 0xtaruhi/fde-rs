use serde::{Deserialize, Serialize};

use super::{Endpoint, Property};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RoutePip {
    pub x: usize,
    pub y: usize,
    pub from_net: String,
    pub to_net: String,
    #[serde(default)]
    pub dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouteSegment {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl RouteSegment {
    pub fn length(&self) -> usize {
        self.x0.abs_diff(self.x1) + self.y0.abs_diff(self.y1)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Net {
    pub name: String,
    #[serde(default)]
    pub driver: Option<Endpoint>,
    #[serde(default)]
    pub sinks: Vec<Endpoint>,
    #[serde(default)]
    pub properties: Vec<Property>,
    #[serde(default)]
    pub route: Vec<RouteSegment>,
    #[serde(default)]
    pub route_pips: Vec<RoutePip>,
    #[serde(default)]
    pub estimated_delay_ns: f64,
    #[serde(default)]
    pub criticality: f64,
}

impl Net {
    pub fn route_length(&self) -> usize {
        self.route.iter().map(RouteSegment::length).sum()
    }
}
