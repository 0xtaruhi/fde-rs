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

/// Board-calibrated per-wire delay term shared by STA estimation and route
/// costing; both must agree or timing reports drift away from route decisions.
pub(crate) const WIRE_DELAY_CONSTANT_NS: f64 = 0.02;
pub(crate) const BEND_DELAY_NS: f64 = 0.05;

/// Estimate the routed wire delay of a segment chain from wire resistance and
/// capacitance, plus a fixed penalty for every direction bend.
pub(crate) fn estimate_segment_delay_ns(route: &[RouteSegment], wire_r: f64, wire_c: f64) -> f64 {
    let length = route.iter().map(RouteSegment::length).sum::<usize>() as f64;
    let bends = route
        .windows(2)
        .filter(|window| match window {
            [lhs, rhs] => (lhs.x0 == lhs.x1) != (rhs.x0 == rhs.x1),
            _ => false,
        })
        .count() as f64;
    length * (wire_r + wire_c + WIRE_DELAY_CONSTANT_NS) + bends * BEND_DELAY_NS
}

/// Estimate the routed wire delay of a pip chain from its pip count.
pub(crate) fn estimate_pip_count_delay_ns(pip_count: usize, wire_r: f64, wire_c: f64) -> f64 {
    pip_count as f64 * (wire_r + wire_c + WIRE_DELAY_CONSTANT_NS)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RoutePip {
    pub x: usize,
    pub y: usize,
    pub from_net: String,
    pub to_net: String,
}

/// The routed branch from a net driver to one concrete sink.
///
/// Multi-fanout nets must retain these branches independently: charging every
/// sink for the union of all PIPs makes timing increasingly pessimistic as
/// fanout grows and can make a detailed timing path impossible to reconcile.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouteSinkPath {
    pub sink: Endpoint,
    #[serde(default)]
    pub route: Vec<RouteSegment>,
    #[serde(default)]
    pub route_pips: Vec<RoutePip>,
    #[serde(default)]
    pub estimated_delay_ns: f64,
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
    /// Per-sink routed branches used by STA. `route` and `route_pips` remain
    /// the de-duplicated whole-net image used for programming and display.
    #[serde(default)]
    pub sink_routes: Vec<RouteSinkPath>,
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

    pub fn route_for_sink(&self, sink: &Endpoint) -> Option<&RouteSinkPath> {
        self.sink_routes
            .iter()
            .filter(|path| {
                path.sink.kind == sink.kind
                    && path.sink.name == sink.name
                    && path.sink.pin == sink.pin
            })
            .max_by(|lhs, rhs| lhs.estimated_delay_ns.total_cmp(&rhs.estimated_delay_ns))
    }
}
