use std::path::{Path, PathBuf};

use crate::report::{LineStageReporter, StageOutput, StageReporter, run_stage_with_reporter};

pub(crate) fn default_sidecar_path(output: &Path) -> PathBuf {
    output.with_extension("sidecar.txt")
}

pub(crate) fn run_cli_stage<T, E, Run, RunWithReporter>(
    stage: &'static str,
    emit_report: bool,
    run: Run,
    run_with_reporter: RunWithReporter,
) -> Result<StageOutput<T>, E>
where
    Run: FnOnce() -> Result<StageOutput<T>, E>,
    RunWithReporter: FnOnce(&mut dyn StageReporter) -> Result<StageOutput<T>, E>,
{
    if emit_report {
        let mut stdout_logger = |line: String| print!("{line}");
        let mut cli_reporter = LineStageReporter::cli(&mut stdout_logger);
        let mut reporter = Some(&mut cli_reporter as &mut dyn StageReporter);
        run_stage_with_reporter(stage, &mut reporter, run, run_with_reporter)
    } else {
        run()
    }
}
