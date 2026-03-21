use crate::ir::{Design, TimingSummary};

pub(crate) fn format_timing_report(design: &Design, summary: &TimingSummary) -> String {
    let mut report = String::new();
    report.push_str("Static Timing Report\n");
    report.push_str(&format!("Design: {}\n", design.name));
    report.push_str(&format!("Stage: {}\n", design.stage));
    report.push_str(&format!(
        "Critical Path: {:.3} ns\n",
        summary.critical_path_ns
    ));
    report.push_str(&format!("Estimated Fmax: {:.2} MHz\n\n", summary.fmax_mhz));
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
