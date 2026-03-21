use crate::{
    ir::{Design, TimingGraph},
    report::{StageOutput, StageReport},
    resource::{Arch, DelayModel},
};

use super::{
    arrival::compute_arrivals,
    error::StaError,
    graph::{build_timing_graph, timing_summary},
    report::format_timing_report,
};

#[derive(Debug, Clone, Default)]
pub struct StaOptions {
    pub arch: Option<Arch>,
    pub delay: Option<DelayModel>,
}

#[derive(Debug, Clone)]
pub struct StaArtifact {
    pub design: Design,
    pub graph: TimingGraph,
    pub report_text: String,
}

pub fn run(mut design: Design, options: &StaOptions) -> Result<StageOutput<StaArtifact>, StaError> {
    design.stage = "timed".to_string();
    let arrivals = compute_arrivals(&design, options.arch.as_ref(), options.delay.as_ref())?;
    let summary = timing_summary(
        &design,
        &arrivals,
        options.arch.as_ref(),
        options.delay.as_ref(),
    )?;
    let graph = build_timing_graph(
        &design,
        &arrivals,
        &summary,
        options.arch.as_ref(),
        options.delay.as_ref(),
    );
    let report_text = format_timing_report(&design, &summary);
    design.timing = Some(summary.clone());

    let mut report = StageReport::new("sta");
    report.push(format!(
        "Computed STA: critical path {:.3} ns, Fmax {:.2} MHz.",
        summary.critical_path_ns, summary.fmax_mhz
    ));

    Ok(StageOutput {
        value: StaArtifact {
            design,
            graph,
            report_text,
        },
        report,
    })
}
