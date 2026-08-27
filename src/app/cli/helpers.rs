use std::path::{Path, PathBuf};
use std::{
    env,
    io::{self, IsTerminal},
};

use crate::report::{
    MessageFormat, StageOutput, StageReporter, TerminalOutputOptions, TerminalStageReporter,
    Verbosity, run_stage_with_reporter,
};

use super::args::{CliColorChoice, CliMessageFormat, CliOutputArgs, CliProgressChoice};

pub(crate) fn default_sidecar_path(output: &Path) -> PathBuf {
    output.with_extension("sidecar.txt")
}

pub(crate) fn run_cli_stage<T, E, Run, RunWithReporter>(
    stage: &'static str,
    output: CliOutputArgs,
    run: Run,
    run_with_reporter: RunWithReporter,
) -> Result<StageOutput<T>, E>
where
    E: std::fmt::Display,
    Run: FnOnce() -> Result<StageOutput<T>, E>,
    RunWithReporter: FnOnce(&mut dyn StageReporter) -> Result<StageOutput<T>, E>,
{
    if matches!(output.message_format, CliMessageFormat::Json) {
        let stdout = io::stdout();
        let interactive = stdout.is_terminal();
        let mut writer = stdout.lock();
        let mut cli_reporter =
            TerminalStageReporter::new(&mut writer, terminal_output_options(output, interactive));
        let mut reporter = Some(&mut cli_reporter as &mut dyn StageReporter);
        run_stage_with_reporter(stage, &mut reporter, run, run_with_reporter)
    } else {
        let stderr = io::stderr();
        let interactive = stderr.is_terminal();
        let mut writer = stderr.lock();
        let mut cli_reporter =
            TerminalStageReporter::new(&mut writer, terminal_output_options(output, interactive));
        let mut reporter = Some(&mut cli_reporter as &mut dyn StageReporter);
        run_stage_with_reporter(stage, &mut reporter, run, run_with_reporter)
    }
}

pub(crate) fn terminal_output_options(
    output: CliOutputArgs,
    interactive: bool,
) -> TerminalOutputOptions {
    let verbosity = if output.quiet {
        Verbosity::Quiet
    } else if output.verbose == 0 {
        Verbosity::Normal
    } else {
        Verbosity::Verbose
    };
    let message_format = match output.message_format {
        CliMessageFormat::Human => MessageFormat::Human,
        CliMessageFormat::Json => MessageFormat::Json,
    };
    let color_environment_allows =
        env::var_os("NO_COLOR").is_none() && env::var("TERM").map_or(true, |term| term != "dumb");
    let color = message_format == MessageFormat::Human
        && color_environment_allows
        && match output.color {
            CliColorChoice::Auto => interactive,
            CliColorChoice::Always => true,
            CliColorChoice::Never => false,
        };
    let progress = message_format == MessageFormat::Human
        && match output.progress {
            CliProgressChoice::Auto => interactive,
            CliProgressChoice::Always => true,
            CliProgressChoice::Never => false,
        };
    TerminalOutputOptions {
        verbosity,
        message_format,
        color,
        progress,
        interactive,
    }
}
