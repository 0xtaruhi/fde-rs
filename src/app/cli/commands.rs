use anyhow::{Context, Result};
use std::{fs, sync::Arc};

use crate::{
    bitgen,
    import::{self, ImportOptions},
    io::{load_design, save_design},
    map::{self, MapOptions},
    normalize::{self, NormalizeOptions},
    orchestrator,
    pack::{self, PackOptions},
    place::{self, PlaceOptions},
    report::print_stage_report,
    resource::{load_arch, load_delay_model},
    route::{self, RouteOptions},
    sta::{self, StaOptions},
};

use super::{
    args::{
        BitgenArgs, Command, ImplArgs, ImportArgs, MapArgs, NormalizeArgs, PackArgs, PlaceArgs,
        RouteArgs, StaArgs,
    },
    helpers::{default_sidecar_path, load_constraints_or_empty, prepare_bitgen},
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
    let result = map::run(
        design,
        &MapOptions {
            lut_size: args.lut_size,
            cell_library: args.cell_library.clone(),
            emit_structural_verilog: args.verilog_output.is_some(),
        },
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
    let result = pack::run(
        design,
        &PackOptions {
            family: args.family,
            capacity: args.capacity,
            cell_library: args.cell_library,
            dcp_library: args.dcp_library,
            config: args.config,
        },
    )?;
    save_design(&result.value, &args.output)?;
    if emit_report {
        print_stage_report(&result.report);
    }
    Ok(())
}

pub(crate) fn run_place(args: PlaceArgs, emit_report: bool) -> Result<()> {
    let design = load_design(&args.input)?;
    let arch = load_arch(&args.arch)?;
    let delay = load_delay_model(args.delay.as_deref())?;
    let constraints = load_constraints_or_empty(args.constraints.as_ref())?;
    let result = place::run(
        design,
        &PlaceOptions {
            arch: Arc::new(arch),
            delay: delay.map(Arc::new),
            constraints,
            mode: args.mode.into(),
            seed: args.seed,
        },
    )?;
    save_design(&result.value, &args.output)?;
    if emit_report {
        print_stage_report(&result.report);
    }
    Ok(())
}

pub(crate) fn run_route(args: RouteArgs, emit_report: bool) -> Result<()> {
    let design = load_design(&args.input)?;
    let arch = load_arch(&args.arch)?;
    let constraints = load_constraints_or_empty(args.constraints.as_ref())?;
    let result = route::run(
        design,
        &RouteOptions {
            arch: Arc::new(arch),
            constraints,
            mode: args.mode.into(),
        },
    )?;
    save_design(&result.value, &args.output)?;
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
    let mut result = sta::run(
        design,
        &StaOptions {
            arch: arch.map(Arc::new),
            delay: delay.map(Arc::new),
        },
    )?;
    if let Some(path) = args.timing_library.as_ref() {
        result
            .report
            .push(format!("Referenced timing library {}", path.display()));
    }
    save_design(&result.value.design, &args.output)?;
    fs::write(&args.report, &result.value.report_text)
        .with_context(|| format!("failed to write {}", args.report.display()))?;
    if emit_report {
        print_stage_report(&result.report);
    }
    Ok(())
}

pub(crate) fn run_bitgen(args: BitgenArgs, emit_report: bool) -> Result<()> {
    let design = load_design(&args.input)?;
    let prepared = prepare_bitgen(design.clone(), args.arch.as_ref(), args.cil.as_ref())?;
    let result = bitgen::run(design, &prepared.options)?;
    let sidecar = args
        .sidecar
        .unwrap_or_else(|| default_sidecar_path(&args.output));
    fs::write(&args.output, &result.value.bytes)
        .with_context(|| format!("failed to write {}", args.output.display()))?;
    fs::write(&sidecar, &result.value.sidecar_text)
        .with_context(|| format!("failed to write {}", sidecar.display()))?;
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
    let report = orchestrator::run(&args.into())?;
    for stage in &report.stages {
        print_stage_report(stage);
    }
    if let Some(report_path) = report.artifacts.get("report") {
        println!("[impl] Wrote report to {}", report_path);
    }
    Ok(())
}
