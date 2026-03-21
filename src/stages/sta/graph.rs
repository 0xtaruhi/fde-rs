use crate::{
    domain::{EndpointKind, TimingPathCategory},
    ir::{
        Cell, Design, DesignIndex, Endpoint, EndpointKey, TimingEdge, TimingGraph, TimingNode,
        TimingPath, TimingSummary,
    },
    resource::{Arch, DelayModel},
};
use std::{cmp::Ordering, collections::HashSet};

use super::{
    delay::{intrinsic_cell_delay_ns, net_delay_ns},
    error::StaError,
    keys::{
        ArrivalMap, TimingEndpoint, TimingKey, cell_arrival_key, endpoint_arrival_key,
        render_endpoint_label, render_timing_key,
    },
};

#[derive(Debug, Clone)]
struct TypedTimingEdge {
    from: TimingKey,
    to: TimingKey,
    delay_ns: f64,
}

#[derive(Debug, Clone)]
enum TraceStep {
    Endpoint(TimingEndpoint),
    Driver {
        endpoint: TimingEndpoint,
        net_delay_ns: f64,
    },
}

pub(crate) fn timing_summary(
    design: &Design,
    index: &DesignIndex<'_>,
    arrival: &ArrivalMap,
    arch: Option<&Arch>,
    delay: Option<&DelayModel>,
) -> Result<TimingSummary, StaError> {
    let mut paths = Vec::new();
    let mut critical: f64 = 0.0;
    for net in &design.nets {
        for sink in &net.sinks {
            let category = path_category(design, index, sink);
            let delay_ns = arrival
                .get(&endpoint_arrival_key(index, sink))
                .copied()
                .unwrap_or(0.0);
            critical = critical.max(delay_ns);
            let trace = trace_path(design, index, arrival, sink, arch, delay);
            paths.push(TimingPath {
                category,
                endpoint: render_endpoint_label(
                    design,
                    index,
                    &TimingEndpoint::from_endpoint(index, sink),
                ),
                delay_ns,
                hops: render_trace_steps(design, index, &trace),
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
    index: &DesignIndex<'_>,
    arrival: &ArrivalMap,
    summary: &TimingSummary,
    arch: Option<&Arch>,
    delay: Option<&DelayModel>,
) -> TimingGraph {
    let mut typed_edges = Vec::<TypedTimingEdge>::new();
    for net in &design.nets {
        let Some(driver) = &net.driver else {
            continue;
        };
        let from = endpoint_arrival_key(index, driver);
        let delay_ns = net_delay_ns(design, index, net, arch, delay);
        for sink in &net.sinks {
            typed_edges.push(TypedTimingEdge {
                from: from.clone(),
                to: endpoint_arrival_key(index, sink),
                delay_ns,
            });
        }
    }
    for (cell_index, cell) in design.cells.iter().enumerate() {
        if cell.is_sequential() {
            continue;
        }
        let cell_id = cell_index.into();
        let cell_delay = intrinsic_cell_delay_ns(cell);
        for input in &cell.inputs {
            let from = cell_arrival_key(cell_id, &input.port);
            for output in &cell.outputs {
                typed_edges.push(TypedTimingEdge {
                    from: from.clone(),
                    to: cell_arrival_key(cell_id, &output.port),
                    delay_ns: cell_delay,
                });
            }
        }
    }

    render_timing_graph(
        design,
        index,
        arrival,
        summary.critical_path_ns,
        typed_edges,
    )
}

fn path_category(design: &Design, index: &DesignIndex<'_>, sink: &Endpoint) -> TimingPathCategory {
    match sink.endpoint_kind() {
        EndpointKind::Cell => {
            if index
                .cell_id(&sink.name)
                .map(|cell_id| index.cell(design, cell_id))
                .is_some_and(Cell::is_sequential)
            {
                TimingPathCategory::RegisterInput
            } else {
                TimingPathCategory::Combinational
            }
        }
        EndpointKind::Port => TimingPathCategory::PrimaryOutput,
        EndpointKind::Unknown => TimingPathCategory::Endpoint,
    }
}

fn trace_path(
    design: &Design,
    index: &DesignIndex<'_>,
    arrival: &ArrivalMap,
    sink: &Endpoint,
    arch: Option<&Arch>,
    delay: Option<&DelayModel>,
) -> Vec<TraceStep> {
    let mut hops = vec![TraceStep::Endpoint(TimingEndpoint::from_endpoint(
        index, sink,
    ))];
    let mut current_endpoint = sink.clone();
    let mut visited = HashSet::<EndpointKey>::new();

    for _ in 0..32 {
        if !visited.insert(current_endpoint.key()) {
            break;
        }
        let Some(net_id) = index.net_for_sink(&current_endpoint) else {
            break;
        };
        let net = index.net(design, net_id);
        let Some(driver) = &net.driver else {
            break;
        };
        hops.push(TraceStep::Driver {
            endpoint: TimingEndpoint::from_endpoint(index, driver),
            net_delay_ns: net_delay_ns(design, index, net, arch, delay),
        });
        if driver.is_port() {
            break;
        }
        let Some(cell_id) = index.cell_id(&driver.name) else {
            break;
        };
        let cell = index.cell(design, cell_id);
        if cell.is_sequential() {
            break;
        }
        let mut best_input = None::<(Endpoint, f64)>;
        for input in &cell.inputs {
            if let Some(input_net_id) = index.net_id(&input.net)
                && let Some(input_driver) = &index.net(design, input_net_id).driver
            {
                let score = arrival
                    .get(&endpoint_arrival_key(index, input_driver))
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
        hops.push(TraceStep::Endpoint(TimingEndpoint::from_endpoint(
            index,
            &current_endpoint,
        )));
        if current_endpoint.is_port() {
            break;
        }
    }

    hops.reverse();
    hops
}

fn render_timing_graph(
    design: &Design,
    index: &DesignIndex<'_>,
    arrival: &ArrivalMap,
    required_ns: f64,
    typed_edges: Vec<TypedTimingEdge>,
) -> TimingGraph {
    let mut nodes = arrival
        .iter()
        .map(|(id, arrival_ns)| TimingNode {
            id: render_timing_key(design, index, id),
            arrival_ns: *arrival_ns,
            required_ns,
            slack_ns: required_ns - *arrival_ns,
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));

    let edges = typed_edges
        .into_iter()
        .map(|edge| TimingEdge {
            from: render_timing_key(design, index, &edge.from),
            to: render_timing_key(design, index, &edge.to),
            delay_ns: edge.delay_ns,
        })
        .collect();

    TimingGraph { nodes, edges }
}

fn render_trace_steps(
    design: &Design,
    index: &DesignIndex<'_>,
    steps: &[TraceStep],
) -> Vec<String> {
    steps
        .iter()
        .map(|step| step.render(design, index))
        .collect()
}

impl TraceStep {
    fn render(&self, design: &Design, index: &DesignIndex<'_>) -> String {
        match self {
            Self::Endpoint(endpoint) => render_endpoint_label(design, index, endpoint),
            Self::Driver {
                endpoint,
                net_delay_ns,
            } => format!(
                "{}[{:.3}ns]",
                render_endpoint_label(design, index, endpoint),
                net_delay_ns
            ),
        }
    }
}
