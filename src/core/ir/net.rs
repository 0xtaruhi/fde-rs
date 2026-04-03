use serde::{Deserialize, Serialize};

use super::{Endpoint, Property};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RouteSegment {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl RouteSegment {
    pub fn new(start: (usize, usize), end: (usize, usize)) -> Self {
        Self {
            x0: start.0,
            y0: start.1,
            x1: end.0,
            y1: end.1,
        }
    }

    pub fn length(&self) -> usize {
        self.x0.abs_diff(self.x1) + self.y0.abs_diff(self.y1)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RoutePip {
    pub x: usize,
    pub y: usize,
    pub from_net: String,
    pub to_net: String,
}

impl RoutePip {
    pub fn new(
        position: (usize, usize),
        from_net: impl Into<String>,
        to_net: impl Into<String>,
    ) -> Self {
        Self {
            x: position.0,
            y: position.1,
            from_net: from_net.into(),
            to_net: to_net.into(),
        }
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
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    pub fn with_driver(mut self, endpoint: Endpoint) -> Self {
        self.driver = Some(endpoint);
        self
    }

    pub fn with_sink(mut self, endpoint: Endpoint) -> Self {
        self.sinks.push(endpoint);
        self
    }

    pub fn with_route_segment(mut self, segment: RouteSegment) -> Self {
        self.route.push(segment);
        self
    }

    pub fn with_route_pip(mut self, pip: RoutePip) -> Self {
        self.route_pips.push(pip);
        self
    }

    pub fn route_length(&self) -> usize {
        if self.route.is_empty() {
            self.route_pips.len()
        } else {
            self.route.iter().map(RouteSegment::length).sum()
        }
    }
}
