use crate::{
    constraints::ClockConstraint,
    ir::{Design, TimingSummary},
};

pub(crate) fn format_timing_report(
    design: &Design,
    summary: &TimingSummary,
    clocks: &[ClockConstraint],
    worst_slack_ns: Option<f64>,
) -> String {
    let mut report = String::new();
    report.push_str("Static Timing Report\n");
    report.push_str(&format!("Design: {}\n", design.name));
    report.push_str(&format!("Stage: {}\n", design.stage));
    report.push_str(&format!(
        "Critical Path: {:.3} ns\n",
        summary.critical_path_ns
    ));
    report.push_str(&format!("Estimated Fmax: {:.2} MHz\n", summary.fmax_mhz));
    if clocks.is_empty() {
        report.push_str("Timing Constraints: none (delay estimate only)\n");
    } else {
        for clock in clocks {
            report.push_str(&format!(
                "Clock: {} on {} ({:.3} ns, {:.2} MHz)\n",
                clock.name,
                clock.port_name,
                clock.period_ns,
                1_000.0 / clock.period_ns
            ));
        }
        if let Some(slack_ns) = worst_slack_ns {
            let status = if slack_ns >= 0.0 { "MET" } else { "VIOLATED" };
            report.push_str(&format!("Worst Slack: {slack_ns:.3} ns ({status})\n"));
        }
    }
    report.push('\n');
    for (index, path) in summary.top_paths.iter().enumerate() {
        report.push_str(&format!(
            "Path {} [{}] {:.3} ns -> {}\n",
            index + 1,
            path.category.as_str(),
            path.delay_ns,
            path.endpoint
        ));
        report.push_str(&format!("  {}\n", path.hops.join(" -> ")));
    }
    report
}
