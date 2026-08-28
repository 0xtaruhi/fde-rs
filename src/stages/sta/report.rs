use crate::ir::{
    Design, TimingConstraintStatus, TimingDelaySource, TimingPath, TimingPointKind, TimingSummary,
};

use super::names::TimingNames;

pub(crate) fn format_timing_report(design: &Design, summary: &TimingSummary) -> String {
    let mut report = String::new();
    let names = TimingNames::new(design);
    report.push_str("FDE Static Timing Analysis\n");
    report.push_str("==========================\n");
    report.push_str(&format!("Design              : {}\n", design.name));
    report.push_str(&format!("Analysis stage      : {}\n", design.stage));
    report.push_str(&format!(
        "Timing status       : {}\n",
        summary.constraint_status.as_str()
    ));
    report.push_str(&format!(
        "Longest path delay  : {:.3} ns\n",
        summary.critical_path_ns
    ));
    report.push_str(&format!(
        "Estimated Fmax      : {:.2} MHz\n",
        summary.fmax_mhz
    ));
    let total_arcs = summary.coverage.modeled_arc_count + summary.coverage.fallback_arc_count;
    let modeled_percent = if total_arcs == 0 {
        100.0
    } else {
        summary.coverage.modeled_arc_count as f64 / total_arcs as f64 * 100.0
    };
    report.push_str(&format!(
        "Delay coverage      : {modeled_percent:.1}% modeled ({} fallback arcs)\n",
        summary.coverage.fallback_arc_count
    ));

    if summary.clocks.is_empty() {
        report.push_str("Timing Constraints: none (delay estimate only)\n");
        report.push_str(
            "WARNING [FDE-STA-0001] No clock constraint was found; this is not a timing pass/fail check.\n",
        );
        report.push_str(
            "  help: add <clock name=\"sys\" port=\"clk\" period=\"10.0\"/> to the constraint file.\n",
        );
    } else {
        report.push_str("\nClock Summary\n");
        report.push_str("-------------\n");
        report.push_str(
            "Clock                Source               Period    Uncertainty    Frequency   Registers\n",
        );
        for clock in &summary.clocks {
            report.push_str(&format!(
                "{:<20} {:<20} {:>8.3} ns {:>8.3} ns {:>8.2} MHz {:>9}\n",
                short_name(&clock.name, 20),
                short_name(&clock.source, 20),
                clock.period_ns,
                clock.setup_uncertainty_ns,
                1_000.0 / clock.period_ns,
                clock.register_count
            ));
        }
        if summary.constraint_status == TimingConstraintStatus::PartiallyConstrained {
            report.push_str("WARNING [FDE-STA-0004] Timing analysis is partially constrained.\n");
            report.push_str(
                "  help: constrain all synchronous data inputs and outputs with set_input_delay/set_output_delay.\n",
            );
        }
        if let Some(worst_slack_ns) = summary.setup.worst_slack_ns {
            report.push_str(&format!(
                "Worst Slack: {worst_slack_ns:.3} ns ({})\n",
                summary.setup.status.as_str()
            ));
        }
    }

    report.push_str("\nSetup Summary\n");
    report.push_str("-------------\n");
    push_optional_time(&mut report, "WNS", summary.setup.worst_slack_ns);
    report.push_str(&format!(
        "{:<24}: {:+.3} ns\n",
        "TNS", summary.setup.total_negative_slack_ns
    ));
    report.push_str(&format!(
        "{:<24}: {} / {}\n",
        "Failing endpoints",
        summary.setup.failing_endpoint_count,
        summary.setup.analyzed_endpoint_count
    ));
    let estimated_min_period_ns = if summary.fmax_mhz > 0.0 {
        1_000.0 / summary.fmax_mhz
    } else {
        0.0
    };
    report.push_str(&format!(
        "{:<24}: {:.3} ns\n",
        "Estimated min period", estimated_min_period_ns
    ));
    report.push_str(&format!(
        "{:<24}: {:.2} MHz\n",
        "Estimated Fmax", summary.fmax_mhz
    ));

    report.push_str("\nConstraint Coverage\n");
    report.push_str("-------------------\n");
    report.push_str(&format!(
        "{:<24}: {} / {} constrained\n",
        "Register endpoints",
        summary.coverage.constrained_register_endpoints,
        summary.coverage.register_endpoints
    ));
    report.push_str(&format!(
        "{:<24}: {} / {} constrained\n",
        "Primary inputs",
        summary.coverage.constrained_primary_inputs,
        summary.coverage.primary_inputs
    ));
    report.push_str(&format!(
        "{:<24}: {} / {} constrained\n",
        "Primary outputs",
        summary.coverage.constrained_primary_outputs,
        summary.coverage.primary_outputs
    ));
    report.push_str(&format!(
        "{:<24}: {}\n",
        "Hold analysis",
        summary.hold.status.as_str()
    ));

    if !summary.path_groups.is_empty() {
        report.push_str("\nPath Group Summary\n");
        report.push_str("------------------\n");
        report.push_str("Group                    Endpoints          WNS          TNS   Failing\n");
        for group in &summary.path_groups {
            let wns = group
                .worst_slack_ns
                .map_or_else(|| "-".to_string(), |value| format!("{value:+.3}"));
            report.push_str(&format!(
                "{:<24} {:>9} {:>12} {:>12.3} {:>9}\n",
                short_name(&group.name, 24),
                group.endpoint_count,
                wns,
                group.total_negative_slack_ns,
                group.failing_endpoint_count
            ));
        }
    }

    for (index, path) in summary.top_paths.iter().enumerate() {
        render_path(&mut report, index + 1, path, summary, &names);
    }
    report
}

fn render_path(
    report: &mut String,
    index: usize,
    path: &TimingPath,
    summary: &TimingSummary,
    names: &TimingNames,
) {
    let status = path.slack_ns.map_or(
        "ESTIMATE",
        |slack| {
            if slack >= 0.0 { "MET" } else { "VIOLATED" }
        },
    );
    report.push_str(&format!("\nPath {index}: {status}"));
    if let Some(slack_ns) = path.slack_ns {
        report.push_str(&format!(" ({slack_ns:+.3} ns)"));
    }
    report.push('\n');
    report.push_str(&"-".repeat(88));
    report.push('\n');
    report.push_str(&format!(
        "Startpoint    : {}\n",
        names.endpoint(&path.startpoint)
    ));
    report.push_str(&format!(
        "Endpoint      : {}\n",
        names.endpoint(&path.endpoint)
    ));
    report.push_str(&format!("Path Group    : {}\n", path.path_group));
    report.push_str(&format!(
        "Path Type     : {:?} (Max, {})\n",
        path.check,
        path.category.as_str()
    ));
    report.push_str(&format!("Logic Levels  : {}\n", path.logic_levels));
    report.push_str(&format!(
        "Launch Clock  : {}\n",
        clock_edge(summary, path.launch_clock.as_deref(), false)
    ));
    report.push_str(&format!(
        "Capture Clock : {}\n",
        clock_edge(summary, path.capture_clock.as_deref(), true)
    ));

    report.push_str("\nData Path\n");
    report.push_str("---------\n");
    report.push_str(
        "Point                                                    Fanout   Incr(ns)   Path(ns)  Model\n",
    );
    for point in &path.points {
        if point.kind == TimingPointKind::SetupCheck {
            continue;
        }
        let fanout = point
            .fanout
            .map_or_else(|| "-".to_string(), |value| value.to_string());
        let label = names.point(point.kind, &point.object);
        report.push_str(&format!(
            "{:<56} {:>6} {:>10.3} {:>10.3}  {}\n",
            short_name(&format_point(point.kind, &label), 56),
            fanout,
            point.increment_ns,
            point.cumulative_ns,
            point_model(point.kind, point.increment_ns, point.delay_source)
        ));
    }

    let delays = PathDelayBreakdown::from_path(path);
    report.push_str("\nDelay Breakdown\n");
    report.push_str("---------------\n");
    push_breakdown_time(report, "Input delay", delays.input_delay_ns);
    push_breakdown_time(report, "Clock-to-Q", delays.clock_to_q_ns);
    push_breakdown_time(report, "Cell delay", delays.cell_ns);
    push_breakdown_time(report, "Net delay", delays.net_ns);
    push_breakdown_time(report, "Library setup time", delays.setup_ns);
    report.push_str(&format!(
        "{:<24}: {:>9.3} ns\n",
        "Data arrival time", path.data_arrival_ns
    ));
    report.push_str(&format!(
        "{:<24}: {:>9.3} ns\n",
        path_delay_label(path),
        path.delay_ns
    ));

    report.push_str("\nTiming Calculation\n");
    report.push_str("------------------\n");
    if let Some(clock_name) = path.capture_clock.as_deref()
        && let Some(clock) = summary.clocks.iter().find(|clock| clock.name == clock_name)
        && path.category == crate::domain::TimingPathCategory::RegisterInput
    {
        report.push_str(&format!(
            "{:<24}: {:+9.3} ns\n",
            "Capture clock edge", clock.period_ns
        ));
        report.push_str(&format!(
            "{:<24}: {:+9.3} ns\n",
            "Clock uncertainty", -clock.setup_uncertainty_ns
        ));
        report.push_str(&format!(
            "{:<24}: {:+9.3} ns\n",
            "Library setup time", -delays.setup_ns
        ));
    }
    if let Some(required_ns) = path.data_required_ns {
        report.push_str(&format!(
            "{:<24}: {:+9.3} ns\n",
            "Data required time", required_ns
        ));
        report.push_str(&format!(
            "{:<24}: {:+9.3} ns\n",
            "Data arrival time", -path.data_arrival_ns
        ));
    } else {
        report.push_str(&format!(
            "{:<24}: {:>9.3} ns\n",
            "Data arrival time", path.data_arrival_ns
        ));
    }
    if let Some(slack_ns) = path.slack_ns {
        report.push_str(&format!(
            "{:<24}: {:+9.3} ns  {}\n",
            "Slack",
            slack_ns,
            if slack_ns >= 0.0 { "MET" } else { "VIOLATED" }
        ));
    }
}

#[derive(Default)]
struct PathDelayBreakdown {
    input_delay_ns: f64,
    clock_to_q_ns: f64,
    cell_ns: f64,
    net_ns: f64,
    setup_ns: f64,
}

impl PathDelayBreakdown {
    fn from_path(path: &TimingPath) -> Self {
        let mut result = Self::default();
        for point in &path.points {
            match point.kind {
                TimingPointKind::Port if point.delay_source == TimingDelaySource::Constraint => {
                    result.input_delay_ns += point.increment_ns;
                }
                TimingPointKind::ClockToQ => result.clock_to_q_ns += point.increment_ns,
                TimingPointKind::CellArc => result.cell_ns += point.increment_ns,
                TimingPointKind::Net => result.net_ns += point.increment_ns,
                TimingPointKind::SetupCheck => result.setup_ns += point.increment_ns,
                TimingPointKind::Port | TimingPointKind::Endpoint => {}
            }
        }
        result
    }
}

fn push_breakdown_time(report: &mut String, label: &str, value: f64) {
    if value > 0.0 {
        report.push_str(&format!("{label:<24}: {value:>9.3} ns\n"));
    }
}

fn path_delay_label(path: &TimingPath) -> &'static str {
    if path.launch_clock.is_some() && path.capture_clock.is_some() {
        "Minimum period"
    } else {
        "Path delay"
    }
}

fn clock_edge(summary: &TimingSummary, name: Option<&str>, capture: bool) -> String {
    let Some(name) = name else {
        return if capture {
            "not applicable".to_string()
        } else {
            "external input".to_string()
        };
    };
    let edge_ns = if capture {
        summary
            .clocks
            .iter()
            .find(|clock| clock.name == name)
            .map_or(0.0, |clock| clock.period_ns)
    } else {
        0.0
    };
    format!("{name} @ {edge_ns:.3} ns")
}

fn push_optional_time(report: &mut String, label: &str, value: Option<f64>) {
    match value {
        Some(value) => report.push_str(&format!("{label:<24}: {value:+.3} ns\n")),
        None => report.push_str(&format!("{label:<24}: N/A\n")),
    }
}

fn format_point(kind: TimingPointKind, object: &str) -> String {
    match kind {
        TimingPointKind::CellArc => format!("CELL  {object}"),
        TimingPointKind::Net => format!("NET   {object}"),
        TimingPointKind::ClockToQ => format!("FF/Q  {object}"),
        TimingPointKind::SetupCheck => format!("SETUP {object}"),
        TimingPointKind::Port | TimingPointKind::Endpoint => object.to_string(),
    }
}

fn delay_source_name(source: TimingDelaySource) -> &'static str {
    match source {
        TimingDelaySource::Constraint => "constraint",
        TimingDelaySource::CellLibrary => "cell-library",
        TimingDelaySource::RoutedRc => "routed-rc",
        TimingDelaySource::DelayTable => "delay-table",
        TimingDelaySource::GeometricEstimate => "geometry",
        TimingDelaySource::Constant => "fallback",
        TimingDelaySource::Unknown => "unknown",
    }
}

fn point_model(
    kind: TimingPointKind,
    increment_ns: f64,
    source: TimingDelaySource,
) -> &'static str {
    if matches!(kind, TimingPointKind::Port | TimingPointKind::Endpoint) && increment_ns == 0.0 {
        "-"
    } else {
        delay_source_name(source)
    }
}

fn short_name(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    if width <= 3 {
        return ".".repeat(width);
    }
    let available = width - 3;
    let prefix_width = available / 2;
    let suffix_width = available - prefix_width;
    let prefix = value.chars().take(prefix_width).collect::<String>();
    let suffix = value
        .chars()
        .rev()
        .take(suffix_width)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("{prefix}...{suffix}")
}

#[cfg(test)]
mod tests {
    use super::short_name;

    #[test]
    fn shortens_long_object_names_without_splitting_unicode() {
        assert_eq!(short_name("abcdefgh", 6), "a...gh");
        assert_eq!(short_name("abc", 6), "abc");
    }
}
