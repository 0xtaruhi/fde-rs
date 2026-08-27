// Clap transfers owned command payloads into these handlers. Retaining that
// ownership boundary avoids borrowing the top-level command enum unnecessarily.
#![allow(clippy::needless_pass_by_value)]

use anyhow::{Context, Result};
use std::{
    fs,
    io::{self, IsTerminal},
    sync::Arc,
};

use crate::{
    app::support::{
        load_constraints_or_empty, load_timing_constraints, place_write_context, prepare_bitgen,
        prepare_route_device_design, route_write_context, sta_write_context,
    },
    bitgen,
    cil::load_cil,
    import::{self, ImportOptions},
    io::{load_design, save_design, save_design_with_context},
    map::{self, MapOptions},
    normalize::{self, NormalizeOptions},
    orchestrator,
    pack::{self, PackOptions},
    place::{self, PlaceOptions},
    report::{StageReporter, TerminalStageReporter},
    resource::{load_arch, load_cell_timing_model, load_delay_model},
    route::{self, RouteOptions},
    sta::{self, StaOptions, StaTimingContext},
};

use super::dispatch::CliExitError;
use super::{
    args::{
        BitgenArgs, CliMessageFormat, CliOutputArgs, Command, ImplArgs, ImportArgs, MapArgs,
        NormalizeArgs, PackArgs, PlaceArgs, RouteArgs, StaArgs,
    },
    helpers::{default_sidecar_path, run_cli_stage, terminal_output_options},
};

pub(crate) fn dispatch_command(command: Command, output: CliOutputArgs) -> Result<()> {
    match command {
        Command::Map(args) => run_map(args, output),
        Command::Pack(args) => run_pack(args, output),
        Command::Place(args) => run_place(args, output),
        Command::Route(args) => run_route(args, output),
        Command::Sta(args) => run_sta(args, output),
        Command::Bitgen(args) => run_bitgen(args, output),
        Command::Normalize(args) => run_normalize(args, output),
        Command::Import(args) => run_import(args, output),
        Command::Impl(args) => run_impl(*args, output),
    }
}

pub(crate) fn run_map(args: MapArgs, output: CliOutputArgs) -> Result<()> {
    let design = map::load_input(&args.input)?;
    let options = MapOptions {
        lut_size: args.lut_size,
        cell_library: args.cell_library.clone(),
        emit_structural_verilog: args.verilog_output.is_some(),
    };
    let design_with_reporter = design.clone();
    let result = run_cli_stage(
        "map",
        output,
        || map::run(design, &options),
        |reporter| map::run_with_reporter(design_with_reporter, &options, reporter),
    )?;
    save_design(&result.value.design, &args.output)?;
    if let Some(path) = args.verilog_output
        && let Some(verilog) = result.value.structural_verilog
    {
        fs::write(&path, verilog).with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

pub(crate) fn run_pack(args: PackArgs, output: CliOutputArgs) -> Result<()> {
    let design = load_design(&args.input)?;
    let options = PackOptions {
        family: args.family,
        capacity: args.capacity,
        cell_library: args.cell_library,
        dcp_library: args.dcp_library,
        config: args.config,
    };
    let design_with_reporter = design.clone();
    let result = run_cli_stage(
        "pack",
        output,
        || pack::run(design, &options),
        |reporter| pack::run_with_reporter(design_with_reporter, &options, reporter),
    )?;
    save_design(&result.value, &args.output)?;
    Ok(())
}

pub(crate) fn run_place(args: PlaceArgs, output: CliOutputArgs) -> Result<()> {
    let design = load_design(&args.input)?;
    let arch = Arc::new(load_arch(&args.arch)?);
    let delay = load_delay_model(args.delay.as_deref())?;
    let constraints = load_constraints_or_empty(args.constraints.as_deref())?;
    let options = PlaceOptions {
        arch: Arc::clone(&arch),
        delay: delay.map(Arc::new),
        constraints: Arc::clone(&constraints),
        mode: args.mode.into(),
        seed: args.seed,
    };
    let design_with_reporter = design.clone();
    let result = run_cli_stage(
        "place",
        output,
        || place::run(design, &options),
        |reporter| place::run_with_reporter(design_with_reporter, &options, reporter),
    )?;
    save_design_with_context(
        &result.value,
        &args.output,
        &place_write_context(arch.as_ref(), constraints.as_ref()),
    )?;
    Ok(())
}

pub(crate) fn run_route(args: RouteArgs, output: CliOutputArgs) -> Result<()> {
    let design = load_design(&args.input)?;
    let arch = Arc::new(load_arch(&args.arch)?);
    let constraints = load_constraints_or_empty(args.constraints.as_deref())?;
    let cil = match args.cil.as_ref() {
        Some(path) => Some(load_cil(path)?),
        None => None,
    };
    let device_design =
        prepare_route_device_design(&design, arch.as_ref(), cil.as_ref(), constraints.as_ref())?;
    let options = RouteOptions {
        arch: Arc::clone(&arch),
        arch_path: args.arch.clone(),
        constraints: Arc::clone(&constraints),
        cil: cil.clone(),
        device_design,
    };
    let design_with_reporter = design.clone();
    let result = run_cli_stage(
        "route",
        output,
        || route::run(design, &options),
        |reporter| route::run_with_reporter(design_with_reporter, &options, reporter),
    )?;
    save_design_with_context(
        &result.value,
        &args.output,
        &route_write_context(
            arch.as_ref(),
            cil.as_ref(),
            constraints.as_ref(),
            args.cil.as_deref(),
        ),
    )?;
    Ok(())
}

pub(crate) fn run_sta(args: StaArgs, output: CliOutputArgs) -> Result<()> {
    let design = load_design(&args.input)?;
    let arch = match args.arch.as_ref() {
        Some(path) => Some(load_arch(path)?),
        None => None,
    };
    let delay = load_delay_model(args.delay.as_deref())?;
    let (constraint_set, sdc_constraints) =
        load_timing_constraints(args.constraints.as_deref(), args.sdc.as_deref())?;
    let cell_timing = args
        .cell_library
        .as_deref()
        .map(load_cell_timing_model)
        .transpose()?;
    let options = StaOptions {
        arch: arch.clone().map(Arc::new),
        delay: delay.map(Arc::new),
    };
    let timing = StaTimingContext {
        clocks: Arc::from(constraint_set.clocks),
        input_delays: Arc::from(sdc_constraints.input_delays),
        output_delays: Arc::from(sdc_constraints.output_delays),
        clock_uncertainties: Arc::from(sdc_constraints.clock_uncertainties),
        cell_timing: cell_timing.map(Arc::new),
    };
    let design_with_reporter = design.clone();
    let result = run_cli_stage(
        "sta",
        output,
        || sta::run_with_timing(design, &options, &timing),
        |reporter| {
            sta::run_with_timing_and_reporter(design_with_reporter, &options, &timing, reporter)
        },
    )?;
    save_design_with_context(
        &result.value.design,
        &args.output,
        &sta_write_context(arch.as_ref()),
    )?;
    fs::write(&args.report, &result.value.report_text)
        .with_context(|| format!("failed to write {}", args.report.display()))?;
    if let Some(json_report) = args.json_report.as_ref() {
        fs::write(json_report, &result.value.report_json)
            .with_context(|| format!("failed to write {}", json_report.display()))?;
    }
    if args.fail_on_timing
        && result.value.design.timing.as_ref().is_some_and(|summary| {
            summary.constraint_status == crate::ir::TimingConstraintStatus::Violated
        })
    {
        return Err(CliExitError::timing_violation(format!(
            "setup timing is violated; inspect {}",
            args.report.display()
        ))
        .into());
    }
    Ok(())
}

pub(crate) fn run_bitgen(args: BitgenArgs, output: CliOutputArgs) -> Result<()> {
    let design = load_design(&args.input)?;
    let prepared = prepare_bitgen(&design, args.arch.as_deref(), args.cil.as_deref())?;
    let design_with_reporter = design.clone();
    let result = run_cli_stage(
        "bitgen",
        output,
        || bitgen::run(design, &prepared.options),
        |reporter| bitgen::run_with_reporter(design_with_reporter, &prepared.options, reporter),
    )?;
    fs::write(&args.output, &result.value.bytes)
        .with_context(|| format!("failed to write {}", args.output.display()))?;
    if args.emit_sidecar || args.sidecar.is_some() {
        let sidecar = args
            .sidecar
            .unwrap_or_else(|| default_sidecar_path(&args.output));
        fs::write(&sidecar, &result.value.sidecar_text)
            .with_context(|| format!("failed to write {}", sidecar.display()))?;
    }
    Ok(())
}

pub(crate) fn run_normalize(args: NormalizeArgs, output: CliOutputArgs) -> Result<()> {
    let design = load_design(&args.input)?;
    let options = NormalizeOptions {
        cell_library: args.cell_library,
        config: args.config,
    };
    let design_with_reporter = design.clone();
    let options_with_reporter = options.clone();
    let result = run_cli_stage(
        "normalize",
        output,
        || normalize::run(design, &options),
        |_| normalize::run(design_with_reporter, &options_with_reporter),
    )?;
    save_design(&result.value, &args.output)?;
    Ok(())
}

pub(crate) fn run_import(args: ImportArgs, output: CliOutputArgs) -> Result<()> {
    let options = ImportOptions::default();
    let input = args.input.clone();
    let result = run_cli_stage(
        "import",
        output,
        || import::run_path(&input, &options),
        |_| import::run_path(&args.input, &options),
    )?;
    save_design(&result.value, &args.output)?;
    Ok(())
}

pub(crate) fn run_impl(args: ImplArgs, output: CliOutputArgs) -> Result<()> {
    if matches!(output.message_format, CliMessageFormat::Json) {
        let stdout = io::stdout();
        let interactive = stdout.is_terminal();
        let mut writer = stdout.lock();
        let mut reporter =
            TerminalStageReporter::new(&mut writer, terminal_output_options(output, interactive));
        run_impl_with_reporter(args, &mut reporter)
    } else {
        let stderr = io::stderr();
        let interactive = stderr.is_terminal();
        let mut writer = stderr.lock();
        let mut reporter =
            TerminalStageReporter::new(&mut writer, terminal_output_options(output, interactive));
        run_impl_with_reporter(args, &mut reporter)
    }
}

fn run_impl_with_reporter(args: ImplArgs, reporter: &mut dyn StageReporter) -> Result<()> {
    let report = orchestrator::run_with_reporter(&args.into(), reporter)?;
    if !matches!(report.status, crate::report::ReportStatus::Success) {
        anyhow::bail!("implementation finished with status {:?}", report.status);
    }
    Ok(())
}
