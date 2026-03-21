use crate::{ir::RouteSegment, route::RouteMode};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RouteMetrics {
    pub(crate) iterations: usize,
    pub(crate) occupied_edges: usize,
    pub(crate) overflow: usize,
    pub(crate) max_edge_usage: usize,
    pub(crate) history_edges: usize,
    pub(crate) total_length: usize,
    pub(crate) overflow_nets: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SearchProfile {
    pub(crate) present_factor: f64,
    pub(crate) history_factor: f64,
    pub(crate) heuristic_factor: f64,
    pub(crate) bend_penalty: f64,
}

pub(crate) fn search_profile(mode: RouteMode, criticality: f64, iteration: usize) -> SearchProfile {
    match mode {
        RouteMode::BreadthFirst => SearchProfile {
            present_factor: 0.0,
            history_factor: 0.0,
            heuristic_factor: 0.0,
            bend_penalty: 0.0,
        },
        RouteMode::Directed => SearchProfile {
            present_factor: 3.0 + iteration as f64 * 0.2,
            history_factor: 1.1 + iteration as f64 * 0.1,
            heuristic_factor: 0.95,
            bend_penalty: 0.14,
        },
        RouteMode::TimingDriven => {
            let criticality = criticality.clamp(0.0, 1.0);
            SearchProfile {
                present_factor: 0.2 + 2.8 * (1.0 - criticality) + iteration as f64 * 0.12,
                history_factor: 0.35 + 1.2 * (1.0 - criticality) + iteration as f64 * 0.08,
                heuristic_factor: 1.2 + 0.45 * criticality,
                bend_penalty: (0.10 - 0.07 * criticality).max(0.01),
            }
        }
    }
}

pub(crate) fn estimate_route_delay(route: &[RouteSegment], wire_r: f64, wire_c: f64) -> f64 {
    let length = route.iter().map(|segment| segment.length()).sum::<usize>() as f64;
    let bends = route
        .windows(2)
        .filter(|window| {
            let a = &window[0];
            let b = &window[1];
            (a.x0 == a.x1) != (b.x0 == b.x1)
        })
        .count() as f64;
    length * (wire_r + wire_c + 0.02) + bends * 0.05
}
