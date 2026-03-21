use crate::{
    ir::{Design, TimingGraph},
    report::{StageOutput, StageReport},
    resource::{SharedArch, SharedDelayModel},
};

use super::{
    arrival::compute_arrivals,
    error::StaError,
    graph::{build_timing_graph, timing_summary},
    report::format_timing_report,
};

#[derive(Debug, Clone, Default)]
pub struct StaOptions {
    pub arch: Option<SharedArch>,
    pub delay: Option<SharedDelayModel>,
}

#[derive(Debug, Clone)]
pub struct StaArtifact {
    pub design: Design,
    pub graph: TimingGraph,
    pub report_text: String,
}

pub fn run(mut design: Design, options: &StaOptions) -> Result<StageOutput<StaArtifact>, StaError> {
    design.stage = "timed".to_string();
    let index = design.index();
    let arrivals = compute_arrivals(&design, options.arch.as_deref(), options.delay.as_deref())?;
    let summary = timing_summary(
        &design,
        &index,
        &arrivals,
        options.arch.as_deref(),
        options.delay.as_deref(),
    )?;
    let graph = build_timing_graph(
        &design,
        &index,
        &arrivals,
        &summary,
        options.arch.as_deref(),
        options.delay.as_deref(),
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
