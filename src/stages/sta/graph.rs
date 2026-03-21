use crate::{
    domain::EndpointKind,
    ir::{Cell, Design, Endpoint, TimingEdge, TimingGraph, TimingNode, TimingPath, TimingSummary},
    resource::{Arch, DelayModel},
};
use std::{cmp::Ordering, collections::HashSet};

use super::{
    delay::{intrinsic_cell_delay_ns, net_delay_ns},
    error::StaError,
    keys::{ArrivalMap, cell_arrival_key, endpoint_arrival_key},
};

pub(crate) fn timing_summary(
    design: &Design,
    arrival: &ArrivalMap,
    arch: Option<&Arch>,
    delay: Option<&DelayModel>,
) -> Result<TimingSummary, StaError> {
    let mut paths = Vec::new();
    let mut critical: f64 = 0.0;
    for net in &design.nets {
        for sink in &net.sinks {
            let category = path_category(design, sink);
            let delay_ns = arrival
                .get(&endpoint_arrival_key(sink))
                .copied()
                .unwrap_or(0.0);
            critical = critical.max(delay_ns);
            paths.push(TimingPath {
                category: category.as_str().to_string(),
                endpoint: format!("{}:{}", sink.name, sink.pin),
                delay_ns,
                hops: trace_path(design, arrival, sink, arch, delay),
            });
        }
    }
    paths.sort_by(|lhs, rhs| {
        rhs.delay_ns
            .partial_cmp(&lhs.delay_ns)
            .unwrap_or(Ordering::Equal)
    });
    paths.truncate(10);

    if !critical.is_finite() {
        return Err(StaError::NonFiniteCriticalPath { value: critical });
    }

    let fmax_mhz = if critical > 0.0 {
        1_000.0 / critical
    } else {
        0.0
    };
    if !fmax_mhz.is_finite() {
        return Err(StaError::NonFiniteFmax { value: fmax_mhz });
    }

    Ok(TimingSummary {
        critical_path_ns: critical,
        fmax_mhz,
        top_paths: paths,
    })
}

pub(crate) fn build_timing_graph(
    design: &Design,
    arrival: &ArrivalMap,
    summary: &TimingSummary,
    arch: Option<&Arch>,
    delay: Option<&DelayModel>,
) -> TimingGraph {
    let mut nodes = arrival
        .iter()
        .map(|(id, arrival_ns)| TimingNode {
            id: id.clone(),
            arrival_ns: *arrival_ns,
            required_ns: summary.critical_path_ns,
            slack_ns: summary.critical_path_ns - *arrival_ns,
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));

    let mut edges = Vec::<TimingEdge>::new();
    for net in &design.nets {
        let Some(driver) = &net.driver else {
            continue;
        };
        let from = endpoint_arrival_key(driver);
        let delay_ns = net_delay_ns(design, net, arch, delay);
        for sink in &net.sinks {
            edges.push(TimingEdge {
                from: from.clone(),
                to: endpoint_arrival_key(sink),
                delay_ns,
            });
        }
    }
    for cell in &design.cells {
        if cell.is_sequential() {
            continue;
        }
        let cell_delay = intrinsic_cell_delay_ns(cell);
        for input in &cell.inputs {
            let from = cell_arrival_key(&cell.name, &input.port);
            for output in &cell.outputs {
                edges.push(TimingEdge {
                    from: from.clone(),
                    to: cell_arrival_key(&cell.name, &output.port),
                    delay_ns: cell_delay,
                });
            }
        }
    }

    TimingGraph { nodes, edges }
}

#[derive(Debug, Clone, Copy)]
enum PathCategory {
    RegisterInput,
    Combinational,
    PrimaryOutput,
    Endpoint,
}

impl PathCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::RegisterInput => "register-input",
            Self::Combinational => "combinational",
            Self::PrimaryOutput => "primary-output",
            Self::Endpoint => "endpoint",
        }
    }
}

fn path_category(design: &Design, sink: &Endpoint) -> PathCategory {
    match sink.endpoint_kind() {
        EndpointKind::Cell => {
            if design
                .cell_lookup(&sink.name)
                .is_some_and(Cell::is_sequential)
            {
                PathCategory::RegisterInput
            } else {
                PathCategory::Combinational
            }
        }
        EndpointKind::Port => PathCategory::PrimaryOutput,
        EndpointKind::Unknown => PathCategory::Endpoint,
    }
}

fn trace_path(
    design: &Design,
    arrival: &ArrivalMap,
    sink: &Endpoint,
    arch: Option<&Arch>,
    delay: Option<&DelayModel>,
) -> Vec<String> {
    let mut hops = vec![format!("{}:{}", sink.name, sink.pin)];
    let mut current_endpoint = sink.clone();
    let mut visited = HashSet::new();

    for _ in 0..32 {
        if !visited.insert(current_endpoint.key()) {
            break;
        }
        let Some(net) = design.nets.iter().find(|net| {
            net.sinks
                .iter()
                .any(|candidate| candidate.key() == current_endpoint.key())
        }) else {
            break;
        };
        let Some(driver) = &net.driver else {
            break;
        };
        hops.push(format!(
            "{}:{}[{:.3}ns]",
            driver.name,
            driver.pin,
            net_delay_ns(design, net, arch, delay)
        ));
        if driver.is_port() {
            break;
        }
        let Some(cell) = design.cell_lookup(&driver.name) else {
            break;
        };
        if cell.is_sequential() {
            break;
        }
        let mut best_input = None::<(Endpoint, f64)>;
        for input in &cell.inputs {
            if let Some(input_net) = design.net_lookup(&input.net)
                && let Some(input_driver) = &input_net.driver
            {
                let score = arrival
                    .get(&endpoint_arrival_key(input_driver))
                    .copied()
                    .unwrap_or(0.0);
                let candidate = input_driver.clone();
                if best_input
                    .as_ref()
                    .map(|(_, best)| score > *best)
                    .unwrap_or(true)
                {
                    best_input = Some((candidate, score));
                }
            }
        }
        let Some((prev, _)) = best_input else {
            break;
        };
        current_endpoint = prev;
        hops.push(format!(
            "{}:{}",
            current_endpoint.name, current_endpoint.pin
        ));
        if current_endpoint.is_port() {
            break;
        }
    }

    hops.reverse();
    hops
}
