use crate::{
    constraints::{SharedClockConstraints, SharedClockUncertainties, SharedIoDelayConstraints},
    ir::{Design, TimingGraph},
    report::{Diagnostic, StageOutput, StageReport, StageReporter, emit_stage_info},
    resource::{CellTimingModel, SharedArch, SharedCellTimingModel, SharedDelayModel},
};
use std::sync::Arc;

use super::{
    arrival::compute_arrivals, constraints::TimingRequirements, error::StaError,
    graph::analyze_timing, report::format_timing_report,
};

#[derive(Debug, Clone, Default)]
pub struct StaOptions {
    pub arch: Option<SharedArch>,
    pub delay: Option<SharedDelayModel>,
}

#[derive(Debug, Clone)]
pub struct StaTimingContext {
    pub clocks: SharedClockConstraints,
    pub input_delays: SharedIoDelayConstraints,
    pub output_delays: SharedIoDelayConstraints,
    pub clock_uncertainties: SharedClockUncertainties,
    pub cell_timing: Option<SharedCellTimingModel>,
}

impl Default for StaTimingContext {
    fn default() -> Self {
        Self {
            clocks: Arc::from([]),
            input_delays: Arc::from([]),
            output_delays: Arc::from([]),
            clock_uncertainties: Arc::from([]),
            cell_timing: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StaArtifact {
    pub design: Design,
    pub graph: TimingGraph,
    pub report_text: String,
    pub report_json: String,
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
    let requirements = TimingRequirements::compile(
        &design,
        &index,
        &timing.clocks,
        &timing.input_delays,
        &timing.output_delays,
        &timing.clock_uncertainties,
        cell_timing,
    )?;
    let arrivals = compute_arrivals(
        &design,
        options.arch.as_deref(),
        options.delay.as_deref(),
        cell_timing,
        &requirements,
    )?;
    emit_stage_info(&mut reporter, "sta", "computed arrival and required times");
    let (summary, graph) = analyze_timing(
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
    let worst_slack_ns = summary.setup.worst_slack_ns;
    let report_text = format_timing_report(&design, &summary);
    let report_json = serde_json::to_string_pretty(&summary)
        .expect("validated timing summary must serialize as JSON");
    design.timing = Some(summary.clone());

    let mut report = StageReport::new("sta");
    report.metric("critical_path_ns", summary.critical_path_ns);
    report.metric("fmax_mhz", summary.fmax_mhz);
    report.metric("top_path_count", summary.top_paths.len());
    report.metric("constrained_clock_count", requirements.clocks.len());
    report.metric("timing_status", summary.constraint_status.as_str());
    report.metric("tns_ns", summary.setup.total_negative_slack_ns);
    report.metric(
        "failing_endpoint_count",
        summary.setup.failing_endpoint_count,
    );
    report.metric(
        "constrained_register_endpoint_count",
        summary.coverage.constrained_register_endpoints,
    );
    report.metric(
        "register_endpoint_count",
        summary.coverage.register_endpoints,
    );
    report.metric("fallback_arc_count", summary.coverage.fallback_arc_count);
    if let Some(worst_slack_ns) = worst_slack_ns {
        report.metric("worst_slack_ns", worst_slack_ns);
        report.metric(
            "timing_met",
            summary.constraint_status == crate::ir::TimingConstraintStatus::Met,
        );
    }
    if let Some(path) = summary.top_paths.first() {
        report.metric("worst_endpoint", path.endpoint.clone());
        report.metric("worst_category", format!("{:?}", path.category));
    }
    report.push(format!(
        "Computed STA: critical path {:.3} ns, Fmax {:.2} MHz.",
        summary.critical_path_ns, summary.fmax_mhz
    ));
    if requirements.clocks.is_empty() {
        report.diagnostic(
            Diagnostic::warning(
                "FDE-STA-0001",
                "No clock constraint was found; timing is an unconstrained estimate.",
            )
            .with_help(
                "add <clock name=\"sys\" port=\"clk\" period=\"10.0\"/> to the constraint file",
            ),
        );
    }
    if summary.coverage.fallback_arc_count > 0 {
        report.diagnostic(
            Diagnostic::warning(
                "FDE-STA-0002",
                format!(
                    "{} timing arc(s) use fallback delay estimates.",
                    summary.coverage.fallback_arc_count
                ),
            )
            .with_help("provide architecture and delay-model resources for sign-off estimates"),
        );
    }
    if summary.constraint_status == crate::ir::TimingConstraintStatus::PartiallyConstrained {
        report.diagnostic(
            Diagnostic::warning(
                "FDE-STA-0004",
                "Timing analysis is only partially constrained.",
            )
            .with_help(
                "add set_input_delay/set_output_delay constraints for all synchronous I/O ports",
            ),
        );
    }

    Ok(StageOutput {
        value: StaArtifact {
            design,
            graph,
            report_text,
            report_json,
        },
        report,
    })
}
