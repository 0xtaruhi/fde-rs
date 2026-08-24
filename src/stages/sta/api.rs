use crate::{
    constraints::SharedClockConstraints,
    ir::{Design, TimingGraph},
    report::{StageOutput, StageReport, StageReporter, emit_stage_info},
    resource::{CellTimingModel, SharedArch, SharedCellTimingModel, SharedDelayModel},
};
use std::sync::Arc;

use super::{
    arrival::compute_arrivals,
    constraints::TimingRequirements,
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
pub struct StaTimingContext {
    pub clocks: SharedClockConstraints,
    pub cell_timing: Option<SharedCellTimingModel>,
}

impl Default for StaTimingContext {
    fn default() -> Self {
        Self {
            clocks: Arc::from([]),
            cell_timing: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StaArtifact {
    pub design: Design,
    pub graph: TimingGraph,
    pub report_text: String,
}

pub fn run(design: Design, options: &StaOptions) -> Result<StageOutput<StaArtifact>, StaError> {
    run_with_timing(design, options, &StaTimingContext::default())
}

pub fn run_with_reporter(
    design: Design,
    options: &StaOptions,
    reporter: &mut dyn StageReporter,
) -> Result<StageOutput<StaArtifact>, StaError> {
    run_with_timing_and_reporter(design, options, &StaTimingContext::default(), reporter)
}

pub fn run_with_timing(
    design: Design,
    options: &StaOptions,
    timing: &StaTimingContext,
) -> Result<StageOutput<StaArtifact>, StaError> {
    run_internal(design, options, timing, None)
}

pub fn run_with_timing_and_reporter(
    design: Design,
    options: &StaOptions,
    timing: &StaTimingContext,
    reporter: &mut dyn StageReporter,
) -> Result<StageOutput<StaArtifact>, StaError> {
    run_internal(design, options, timing, Some(reporter))
}

fn run_internal(
    mut design: Design,
    options: &StaOptions,
    timing: &StaTimingContext,
    mut reporter: Option<&mut dyn StageReporter>,
) -> Result<StageOutput<StaArtifact>, StaError> {
    design.stage = "timed".to_string();
    emit_stage_info(
        &mut reporter,
        "sta",
        format!(
            "building STA model for {} nets and {} cells",
            design.nets.len(),
            design.cells.len()
        ),
    );
    let index = design.index();
    let default_cell_timing = CellTimingModel::default();
    let cell_timing = timing
        .cell_timing
        .as_deref()
        .unwrap_or(&default_cell_timing);
    let requirements = TimingRequirements::compile(&design, &index, &timing.clocks, cell_timing)?;
    let arrivals = compute_arrivals(
        &design,
        options.arch.as_deref(),
        options.delay.as_deref(),
        cell_timing,
    )?;
    emit_stage_info(&mut reporter, "sta", "computed arrival and required times");
    let summary = timing_summary(
        &design,
        &index,
        &arrivals,
        &requirements,
        options.arch.as_deref(),
        options.delay.as_deref(),
    )?;
    emit_stage_info(
        &mut reporter,
        "sta",
        format!(
            "worst path {:.3} ns, estimated Fmax {:.2} MHz",
            summary.critical_path_ns, summary.fmax_mhz
        ),
    );
    let graph = build_timing_graph(
        &design,
        &index,
        &arrivals,
        &summary,
        &requirements,
        options.arch.as_deref(),
        options.delay.as_deref(),
    );
    let worst_slack_ns = requirements.worst_slack_ns(&arrivals);
    let report_text = format_timing_report(&design, &summary, &requirements.clocks, worst_slack_ns);
    design.timing = Some(summary.clone());

    let mut report = StageReport::new("sta");
    report.metric("critical_path_ns", summary.critical_path_ns);
    report.metric("fmax_mhz", summary.fmax_mhz);
    report.metric("top_path_count", summary.top_paths.len());
    report.metric("constrained_clock_count", requirements.clocks.len());
    if let Some(worst_slack_ns) = worst_slack_ns {
        report.metric("worst_slack_ns", worst_slack_ns);
        report.metric("timing_met", worst_slack_ns >= 0.0);
    }
    if let Some(path) = summary.top_paths.first() {
        report.metric("worst_endpoint", path.endpoint.clone());
        report.metric("worst_category", format!("{:?}", path.category));
    }
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
