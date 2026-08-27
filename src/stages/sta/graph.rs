use crate::{
    domain::TimingPathCategory,
    ir::{
        Design, DesignIndex, Endpoint, EndpointKey, EndpointTarget, TimingCheckKind,
        TimingCheckSummary, TimingClockSummary, TimingConstraintStatus, TimingCoverage,
        TimingDelaySource, TimingEdge, TimingGraph, TimingNode, TimingPath, TimingPathGroupSummary,
        TimingPathPoint, TimingPointKind, TimingSummary,
    },
    resource::{Arch, DelayModel},
};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashSet},
};

use super::{
    constraints::TimingRequirements,
    delay::{cell_delay_estimate, cell_input_is_functional, net_delay_estimate},
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
    kind: TimingPointKind,
    object: String,
    delay_source: TimingDelaySource,
    fanout: Option<usize>,
}

#[derive(Debug, Clone, Default)]
struct PathTrace {
    start: Option<TimingKey>,
    arcs: Vec<TypedTimingEdge>,
    logic_levels: usize,
}

pub(crate) fn analyze_timing(
    design: &Design,
    index: &DesignIndex<'_>,
    arrival: &ArrivalMap,
    requirements: &TimingRequirements,
    arch: Option<&Arch>,
    delay: Option<&DelayModel>,
) -> Result<(TimingSummary, TimingGraph), StaError> {
    let edges = collect_timing_edges(design, index, arch, delay);
    let summary = timing_summary(design, index, arrival, requirements, arch, delay, &edges)?;
    let required = compute_required_times(&edges, arrival, requirements);
    let graph = render_timing_graph(design, index, arrival, &required, edges);
    Ok((summary, graph))
}

fn timing_summary(
    design: &Design,
    index: &DesignIndex<'_>,
    arrival: &ArrivalMap,
    requirements: &TimingRequirements,
    arch: Option<&Arch>,
    delay: Option<&DelayModel>,
    typed_edges: &[TypedTimingEdge],
) -> Result<TimingSummary, StaError> {
    let mut paths = Vec::new();
    let mut critical: f64 = 0.0;
    let mut minimum_clock_period: f64 = 0.0;
    for net in &design.nets {
        for sink in &net.sinks {
            if !is_path_endpoint(design, index, sink) {
                continue;
            }
            let category = path_category(index, sink);
            let key = endpoint_arrival_key(index, sink);
            let data_arrival_ns = arrival.get(&key).copied().unwrap_or(0.0);
            let setup_ns = requirements.setup_ns(&key);
            let delay_ns = data_arrival_ns + setup_ns;
            critical = critical.max(delay_ns);
            let trace = trace_path(design, index, arrival, sink, arch, delay);
            if category == TimingPathCategory::RegisterInput {
                let external_input_delay = trace
                    .start
                    .as_ref()
                    .filter(|start| {
                        matches!(
                            start,
                            TimingKey::Port(_) | TimingKey::Endpoint(TimingEndpoint::Port { .. })
                        )
                    })
                    .and_then(|start| arrival.get(start))
                    .copied()
                    .unwrap_or(0.0);
                minimum_clock_period = minimum_clock_period.max(delay_ns - external_input_delay);
            }
            let points = render_trace_points(
                design,
                index,
                arrival,
                &trace,
                setup_ns,
                category == TimingPathCategory::RegisterInput,
            );
            let startpoint = trace
                .start
                .as_ref()
                .map_or_else(String::new, |key| render_timing_label(design, index, key));
            let required_ns = requirements.required_ns(&key);
            let slack_ns = requirements.slack_ns(&key, data_arrival_ns);
            let capture_clock = requirements
                .clock_name_for_endpoint(&key)
                .map(ToString::to_string);
            let launch_clock = trace.start.as_ref().and_then(|start| match start {
                TimingKey::Endpoint(TimingEndpoint::Cell { cell_id, .. }) => requirements
                    .clock_name_for_cell(*cell_id)
                    .map(ToString::to_string),
                _ => None,
            });
            let path_group = match category {
                TimingPathCategory::RegisterInput => capture_clock
                    .clone()
                    .unwrap_or_else(|| "unconstrained".to_string()),
                TimingPathCategory::PrimaryOutput => "outputs".to_string(),
                _ => "default".to_string(),
            };
            paths.push(TimingPath {
                category,
                check: TimingCheckKind::Setup,
                startpoint,
                endpoint: render_endpoint_label(
                    design,
                    index,
                    &TimingEndpoint::from_endpoint(index, sink),
                ),
                path_group,
                launch_clock,
                capture_clock,
                delay_ns,
                data_arrival_ns,
                data_required_ns: required_ns,
                slack_ns,
                logic_levels: trace.logic_levels,
                hops: render_trace_hops(design, index, &trace),
                points,
            });
        }
    }

    let path_groups = summarize_path_groups(&paths);
    paths.sort_by(|lhs, rhs| match (lhs.slack_ns, rhs.slack_ns) {
        (Some(lhs), Some(rhs)) => lhs.total_cmp(&rhs),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => rhs.delay_ns.total_cmp(&lhs.delay_ns),
    });
    paths.truncate(10);

    if !critical.is_finite() {
        return Err(StaError::NonFiniteCriticalPath { value: critical });
    }

    let fmax_reference = if minimum_clock_period > 0.0 {
        minimum_clock_period
    } else {
        critical
    };
    let fmax_mhz = if fmax_reference > 0.0 {
        1_000.0 / fmax_reference
    } else {
        0.0
    };
    if !fmax_mhz.is_finite() {
        return Err(StaError::NonFiniteFmax { value: fmax_mhz });
    }

    let setup_slacks = requirements
        .constrained_endpoint_slacks(arrival)
        .map(|(_, slack)| slack)
        .collect::<Vec<_>>();
    let worst_slack_ns = setup_slacks.iter().copied().min_by(f64::total_cmp);
    let total_negative_slack_ns = normalize_zero(
        setup_slacks
            .iter()
            .copied()
            .filter(|slack| *slack < 0.0)
            .sum(),
    );
    let failing_endpoint_count = setup_slacks.iter().filter(|slack| **slack < 0.0).count();
    let primary_input_count = design
        .ports
        .iter()
        .filter(|port| port.direction.is_input_like() && !requirements.is_clock_port(&port.name))
        .count();
    let primary_output_count = design
        .ports
        .iter()
        .filter(|port| port.direction.is_output_like())
        .count();
    let incomplete_coverage = requirements.constrained_register_endpoint_count()
        < requirements.register_endpoint_count()
        || requirements.constrained_primary_input_count() < primary_input_count
        || requirements.constrained_primary_output_count() < primary_output_count;
    let constraint_status = if requirements.clocks.is_empty() {
        TimingConstraintStatus::Unconstrained
    } else if failing_endpoint_count > 0 {
        TimingConstraintStatus::Violated
    } else if incomplete_coverage {
        TimingConstraintStatus::PartiallyConstrained
    } else {
        TimingConstraintStatus::Met
    };
    let fallback_arc_count = typed_edges
        .iter()
        .filter(|edge| {
            matches!(
                edge.delay_source,
                TimingDelaySource::GeometricEstimate
                    | TimingDelaySource::Constant
                    | TimingDelaySource::Unknown
            )
        })
        .count();
    let modeled_arc_count = typed_edges.len().saturating_sub(fallback_arc_count);
    let clocks = requirements
        .clocks
        .iter()
        .map(|clock| TimingClockSummary {
            name: clock.name.clone(),
            source: clock.port_name.clone(),
            period_ns: clock.period_ns,
            setup_uncertainty_ns: requirements.clock_uncertainty_ns(&clock.name),
            register_count: requirements.register_count_for_clock(&clock.name),
        })
        .collect();
    let coverage = TimingCoverage {
        register_endpoints: requirements.register_endpoint_count(),
        constrained_register_endpoints: requirements.constrained_register_endpoint_count(),
        primary_inputs: primary_input_count,
        constrained_primary_inputs: requirements.constrained_primary_input_count(),
        primary_outputs: primary_output_count,
        constrained_primary_outputs: requirements.constrained_primary_output_count(),
        modeled_arc_count,
        fallback_arc_count,
    };

    Ok(TimingSummary {
        constraint_status,
        critical_path_ns: critical,
        fmax_mhz,
        setup: TimingCheckSummary {
            status: constraint_status,
            worst_slack_ns,
            total_negative_slack_ns,
            failing_endpoint_count,
            analyzed_endpoint_count: setup_slacks.len(),
        },
        hold: TimingCheckSummary {
            status: TimingConstraintStatus::NotAnalyzed,
            ..TimingCheckSummary::default()
        },
        coverage,
        clocks,
        path_groups,
        top_paths: paths,
    })
}

fn summarize_path_groups(paths: &[TimingPath]) -> Vec<TimingPathGroupSummary> {
    let mut groups = BTreeMap::<String, TimingPathGroupSummary>::new();
    for path in paths {
        let group =
            groups
                .entry(path.path_group.clone())
                .or_insert_with(|| TimingPathGroupSummary {
                    name: path.path_group.clone(),
                    ..TimingPathGroupSummary::default()
                });
        group.endpoint_count += 1;
        if let Some(slack_ns) = path.slack_ns {
            group.worst_slack_ns = Some(
                group
                    .worst_slack_ns
                    .map_or(slack_ns, |worst| worst.min(slack_ns)),
            );
            if slack_ns < 0.0 {
                group.total_negative_slack_ns =
                    normalize_zero(group.total_negative_slack_ns + slack_ns);
                group.failing_endpoint_count += 1;
            }
        }
    }
    groups.into_values().collect()
}

fn normalize_zero(value: f64) -> f64 {
    if value.abs() < f64::EPSILON {
        0.0
    } else {
        value
    }
}

fn collect_timing_edges(
    design: &Design,
    index: &DesignIndex<'_>,
    arch: Option<&Arch>,
    delay: Option<&DelayModel>,
) -> Vec<TypedTimingEdge> {
    let mut typed_edges = Vec::<TypedTimingEdge>::new();
    for net in &design.nets {
        let Some(driver) = &net.driver else {
            continue;
        };
        let from = endpoint_arrival_key(index, driver);
        for sink in &net.sinks {
            let estimate = net_delay_estimate(design, index, net, Some(sink), arch, delay);
            typed_edges.push(TypedTimingEdge {
                from: from.clone(),
                to: endpoint_arrival_key(index, sink),
                delay_ns: estimate.delay_ns,
                kind: TimingPointKind::Net,
                object: net.name.clone(),
                delay_source: estimate.source,
                fanout: Some(net.sinks.len()),
            });
        }
    }
    for (cell_index, cell) in design.cells.iter().enumerate() {
        if cell.is_sequential() {
            continue;
        }
        let cell_id = cell_index.into();
        let cell_delay = cell_delay_estimate(cell, delay);
        for input in &cell.inputs {
            if !cell_input_is_functional(cell, &input.port) {
                continue;
            }
            let from = cell_arrival_key(cell_id, &input.port);
            for output in &cell.outputs {
                typed_edges.push(TypedTimingEdge {
                    from: from.clone(),
                    to: cell_arrival_key(cell_id, &output.port),
                    delay_ns: cell_delay.delay_ns,
                    kind: TimingPointKind::CellArc,
                    object: cell.name.clone(),
                    delay_source: cell_delay.source,
                    fanout: None,
                });
            }
        }
    }
    typed_edges
}

type RequiredMap = std::collections::BTreeMap<TimingKey, f64>;

/// Backward required-time propagation.
///
/// Constrained register inputs are seeded from their capture-clock period
/// minus setup time. Other nodes use the global worst arrival as an
/// unconstrained reference period. Requirements are then relaxed upstream through
/// `required(from) <= required(to) - delay(edge)` until they settle. The
/// result is a real per-node required time, so slacks distinguish parallel
/// branches instead of reporting the global critical path everywhere.
fn compute_required_times(
    edges: &[TypedTimingEdge],
    arrival: &ArrivalMap,
    requirements: &TimingRequirements,
) -> RequiredMap {
    let mut required = RequiredMap::new();
    let worst_arrival = arrival.values().copied().fold(f64::NEG_INFINITY, f64::max);
    for key in arrival.keys() {
        required.insert(
            key.clone(),
            requirements.required_ns(key).unwrap_or(worst_arrival),
        );
    }

    let mut changed = true;
    while changed {
        changed = false;
        for edge in edges {
            let Some(target_required) = required.get(&edge.to).copied() else {
                continue;
            };
            let candidate = target_required - edge.delay_ns;
            let entry = required.entry(edge.from.clone()).or_insert(candidate);
            if candidate < *entry {
                *entry = candidate;
                changed = true;
            }
        }
    }
    required
}

fn is_path_endpoint(design: &Design, index: &DesignIndex<'_>, sink: &Endpoint) -> bool {
    match index.resolve_endpoint(sink) {
        EndpointTarget::Port(port_id) => index.port(design, port_id).direction.is_output_like(),
        EndpointTarget::Cell(cell_id) => {
            let cell = index.cell(design, cell_id);
            cell.primitive_kind().is_register_data_pin(&sink.pin)
        }
        EndpointTarget::Unknown => false,
    }
}

fn path_category(index: &DesignIndex<'_>, sink: &Endpoint) -> TimingPathCategory {
    match index.resolve_endpoint(sink) {
        EndpointTarget::Cell(_) => TimingPathCategory::RegisterInput,
        EndpointTarget::Port(_) => TimingPathCategory::PrimaryOutput,
        EndpointTarget::Unknown => TimingPathCategory::Endpoint,
    }
}

fn trace_path(
    design: &Design,
    index: &DesignIndex<'_>,
    arrival: &ArrivalMap,
    sink: &Endpoint,
    arch: Option<&Arch>,
    delay: Option<&DelayModel>,
) -> PathTrace {
    let mut trace = PathTrace::default();
    let mut current_endpoint = sink.clone();
    let mut visited = HashSet::<EndpointKey>::new();

    loop {
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
        let estimate = net_delay_estimate(design, index, net, Some(&current_endpoint), arch, delay);
        trace.arcs.push(TypedTimingEdge {
            from: endpoint_arrival_key(index, driver),
            to: endpoint_arrival_key(index, &current_endpoint),
            kind: TimingPointKind::Net,
            object: net.name.clone(),
            delay_ns: estimate.delay_ns,
            delay_source: estimate.source,
            fanout: Some(net.sinks.len()),
        });
        if driver.is_port() {
            trace.start = Some(endpoint_arrival_key(index, driver));
            break;
        }
        let Some(cell_id) = index.cell_id(&driver.name) else {
            trace.start = Some(endpoint_arrival_key(index, driver));
            break;
        };
        let cell = index.cell(design, cell_id);
        if cell.is_sequential() {
            trace.start = Some(endpoint_arrival_key(index, driver));
            break;
        }
        let mut best_input = None::<(Endpoint, f64)>;
        for input in &cell.inputs {
            if !cell_input_is_functional(cell, &input.port) {
                continue;
            }
            let candidate = Endpoint::cell(&cell.name, &input.port);
            let score = arrival
                .get(&endpoint_arrival_key(index, &candidate))
                .copied()
                .unwrap_or(0.0);
            if best_input.as_ref().is_none_or(|(_, best)| score > *best) {
                best_input = Some((candidate, score));
            }
        }
        let Some((input_endpoint, _)) = best_input else {
            trace.start = Some(endpoint_arrival_key(index, driver));
            break;
        };
        let cell_delay = cell_delay_estimate(cell, delay);
        trace.arcs.push(TypedTimingEdge {
            from: endpoint_arrival_key(index, &input_endpoint),
            to: endpoint_arrival_key(index, driver),
            kind: TimingPointKind::CellArc,
            object: cell.name.clone(),
            delay_ns: cell_delay.delay_ns,
            delay_source: cell_delay.source,
            fanout: None,
        });
        trace.logic_levels += 1;
        current_endpoint = input_endpoint;
    }

    trace.arcs.reverse();
    trace
}

fn render_timing_graph(
    design: &Design,
    index: &DesignIndex<'_>,
    arrival: &ArrivalMap,
    required: &RequiredMap,
    typed_edges: Vec<TypedTimingEdge>,
) -> TimingGraph {
    let fallback_required = arrival.values().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut nodes = arrival
        .iter()
        .map(|(id, arrival_ns)| {
            let required_ns = required.get(id).copied().unwrap_or(fallback_required);
            TimingNode {
                id: render_timing_key(design, index, id),
                arrival_ns: *arrival_ns,
                required_ns,
                slack_ns: required_ns - *arrival_ns,
            }
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

fn render_trace_points(
    design: &Design,
    index: &DesignIndex<'_>,
    arrival: &ArrivalMap,
    trace: &PathTrace,
    setup_ns: f64,
    register_endpoint: bool,
) -> Vec<TimingPathPoint> {
    let Some(start) = trace.start.as_ref() else {
        return Vec::new();
    };
    let start_arrival = arrival.get(start).copied().unwrap_or(0.0);
    let mut cumulative_ns = start_arrival;
    let start_kind = match start {
        TimingKey::Port(_) | TimingKey::Endpoint(TimingEndpoint::Port { .. }) => {
            TimingPointKind::Port
        }
        TimingKey::Endpoint(TimingEndpoint::Cell { cell_id, .. })
            if index.cell(design, *cell_id).is_sequential() =>
        {
            TimingPointKind::ClockToQ
        }
        _ => TimingPointKind::Endpoint,
    };
    let mut points = vec![TimingPathPoint {
        kind: start_kind,
        object: render_timing_label(design, index, start),
        increment_ns: start_arrival,
        cumulative_ns,
        delay_source: if start_kind == TimingPointKind::ClockToQ {
            TimingDelaySource::CellLibrary
        } else if start_arrival > 0.0 {
            TimingDelaySource::Constraint
        } else {
            TimingDelaySource::Constant
        },
        ..TimingPathPoint::default()
    }];
    for arc in &trace.arcs {
        cumulative_ns += arc.delay_ns;
        points.push(TimingPathPoint {
            kind: arc.kind,
            object: if arc.kind == TimingPointKind::Net {
                format!(
                    "{} -> {}",
                    arc.object,
                    render_timing_label(design, index, &arc.to)
                )
            } else {
                arc.object.clone()
            },
            increment_ns: arc.delay_ns,
            cumulative_ns,
            fanout: arc.fanout,
            delay_source: arc.delay_source,
        });
    }
    if register_endpoint {
        points.push(TimingPathPoint {
            kind: TimingPointKind::SetupCheck,
            object: "setup check".to_string(),
            increment_ns: setup_ns,
            cumulative_ns: cumulative_ns + setup_ns,
            delay_source: TimingDelaySource::CellLibrary,
            ..TimingPathPoint::default()
        });
    }
    points
}

fn render_trace_hops(design: &Design, index: &DesignIndex<'_>, trace: &PathTrace) -> Vec<String> {
    let mut hops = trace
        .start
        .as_ref()
        .map(|key| vec![render_timing_label(design, index, key)])
        .unwrap_or_default();
    hops.extend(trace.arcs.iter().map(|arc| {
        format!(
            "{}[{:.3}ns]",
            render_timing_label(design, index, &arc.to),
            arc.delay_ns
        )
    }));
    hops
}

fn render_timing_label(design: &Design, index: &DesignIndex<'_>, key: &TimingKey) -> String {
    match key {
        TimingKey::Port(port_id) => index.port(design, *port_id).name.clone(),
        TimingKey::Endpoint(endpoint) => render_endpoint_label(design, index, endpoint),
        TimingKey::Net(net_id) => index.net(design, *net_id).name.clone(),
    }
}
