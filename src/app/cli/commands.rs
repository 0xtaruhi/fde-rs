// Clap transfers owned command payloads into these handlers. Retaining that
// ownership boundary avoids borrowing the top-level command enum unnecessarily.
#![allow(clippy::needless_pass_by_value)]

use anyhow::{Context, Result};
use std::{fs, sync::Arc};

use crate::{
    app::support::{
        load_constraint_set_or_empty, load_constraints_or_empty, place_write_context,
        prepare_bitgen, prepare_route_device_design, route_write_context, sta_write_context,
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
    report::print_stage_report,
    resource::{load_arch, load_cell_timing_model, load_delay_model},
    route::{self, RouteOptions},
    sta::{self, StaOptions, StaTimingContext},
};

use super::{
    args::{
        BitgenArgs, Command, ImplArgs, ImportArgs, MapArgs, NormalizeArgs, PackArgs, PlaceArgs,
        RouteArgs, StaArgs,
    },
    helpers::{default_sidecar_path, run_cli_stage},
};

pub(crate) fn dispatch_command(command: Command) -> Result<()> {
    match command {
        Command::Map(args) => run_map(args, true),
        Command::Pack(args) => run_pack(args, true),
        Command::Place(args) => run_place(args, true),
        Command::Route(args) => run_route(args, true),
        Command::Sta(args) => run_sta(args, true),
        Command::Bitgen(args) => run_bitgen(args, true),
        Command::Normalize(args) => run_normalize(args, true),
        Command::Import(args) => run_import(args, true),
        Command::Impl(args) => run_impl(*args),
    }
}

pub(crate) fn run_map(args: MapArgs, emit_report: bool) -> Result<()> {
    let design = map::load_input(&args.input)?;
    let options = MapOptions {
        lut_size: args.lut_size,
        cell_library: args.cell_library.clone(),
        emit_structural_verilog: args.verilog_output.is_some(),
    };
    let design_with_reporter = design.clone();
    let result = run_cli_stage(
        "map",
        emit_report,
        || map::run(design, &options),
        |reporter| map::run_with_reporter(design_with_reporter, &options, reporter),
    )?;
    save_design(&result.value.design, &args.output)?;
    if let Some(path) = args.verilog_output
        && let Some(verilog) = result.value.structural_verilog
    {
        fs::write(&path, verilog).with_context(|| format!("failed to write {}", path.display()))?;
    }
    if emit_report {
        print_stage_report(&result.report);
    }
    Ok(())
}

pub(crate) fn run_pack(args: PackArgs, emit_report: bool) -> Result<()> {
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
        emit_report,
        || pack::run(design, &options),
        |reporter| pack::run_with_reporter(design_with_reporter, &options, reporter),
    )?;
    save_design(&result.value, &args.output)?;
    if emit_report {
        print_stage_report(&result.report);
    }
    Ok(())
}

pub(crate) fn run_place(args: PlaceArgs, emit_report: bool) -> Result<()> {
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
        emit_report,
        || place::run(design, &options),
        |reporter| place::run_with_reporter(design_with_reporter, &options, reporter),
    )?;
    save_design_with_context(
        &result.value,
        &args.output,
        &place_write_context(arch.as_ref(), constraints.as_ref()),
    )?;
    if emit_report {
        print_stage_report(&result.report);
    }
    Ok(())
}

pub(crate) fn run_route(args: RouteArgs, emit_report: bool) -> Result<()> {
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
        emit_report,
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
    if emit_report {
        print_stage_report(&result.report);
    }
    Ok(())
}

pub(crate) fn run_sta(args: StaArgs, emit_report: bool) -> Result<()> {
    let design = load_design(&args.input)?;
    let arch = match args.arch.as_ref() {
        Some(path) => Some(load_arch(path)?),
        None => None,
    };
    let delay = load_delay_model(args.delay.as_deref())?;
    let constraint_set = load_constraint_set_or_empty(args.constraints.as_deref())?;
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
        cell_timing: cell_timing.map(Arc::new),
    };
    let design_with_reporter = design.clone();
    let result = run_cli_stage(
        "sta",
        emit_report,
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
    if emit_report {
        print_stage_report(&result.report);
    }
    Ok(())
}

pub(crate) fn run_bitgen(args: BitgenArgs, emit_report: bool) -> Result<()> {
    let design = load_design(&args.input)?;
    let prepared = prepare_bitgen(&design, args.arch.as_deref(), args.cil.as_deref())?;
    let design_with_reporter = design.clone();
    let result = run_cli_stage(
        "bitgen",
        emit_report,
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
    if emit_report {
        print_stage_report(&result.report);
    }
    Ok(())
}

pub(crate) fn run_normalize(args: NormalizeArgs, emit_report: bool) -> Result<()> {
    let design = load_design(&args.input)?;
    let result = normalize::run(
        design,
        &NormalizeOptions {
            cell_library: args.cell_library,
            config: args.config,
        },
    )?;
    save_design(&result.value, &args.output)?;
    if emit_report {
        print_stage_report(&result.report);
    }
    Ok(())
}

pub(crate) fn run_import(args: ImportArgs, emit_report: bool) -> Result<()> {
    let result = import::run_path(&args.input, &ImportOptions::default())?;
    save_design(&result.value, &args.output)?;
    if emit_report {
        print_stage_report(&result.report);
    }
    Ok(())
}

pub(crate) fn run_impl(args: ImplArgs) -> Result<()> {
    let mut stdout_logger = |line: String| print!("{line}");
    let mut reporter = crate::report::LineStageReporter::cli(&mut stdout_logger);
    let report = orchestrator::run_with_reporter(&args.into(), &mut reporter)?;
    for stage in &report.stages {
        print_stage_report(stage);
    }
    if let Some(summary_path) = report.artifacts.get("summary") {
        println!("[impl] Wrote summary to {summary_path}");
    }
    if let Some(log_path) = report.artifacts.get("log") {
        println!("[impl] Wrote log to {log_path}");
    }
    if let Some(report_path) = report.artifacts.get("report") {
        println!("[impl] Wrote report to {report_path}");
    }
    Ok(())
}
