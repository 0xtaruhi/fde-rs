use anyhow::{Result, anyhow};
use clap::Parser;

use super::{
    args::{
        BitgenArgs, CompatBitgenArgs, CompatImportArgs, CompatMapArgs, CompatNlfinerArgs,
        CompatPackArgs, CompatPlaceArgs, CompatRouteArgs, CompatStaArgs, ImportArgs, MapArgs,
        NormalizeArgs, PackArgs, PlaceArgs, RouteArgs, StaArgs,
    },
    commands::{
        run_bitgen, run_import, run_map, run_normalize, run_pack, run_place, run_route, run_sta,
    },
    helpers::{compat_place_mode, compat_route_mode},
};

pub fn run_map_wrapper() -> Result<()> {
    let args = CompatMapArgs::parse();
    run_map(
        MapArgs {
            input: args.input,
            output: args.output,
            cell_library: args.cell_library,
            lut_size: args.lut_size,
            verilog_output: args.verilog_output,
        },
        args.emit,
    )
}

pub fn run_pack_wrapper() -> Result<()> {
    let args = CompatPackArgs::parse();
    run_pack(
        PackArgs {
            input: args.input,
            output: args.output,
            family: args.family,
            capacity: args.capacity,
            cell_library: args.cell_library,
            dcp_library: args.dcp_library,
            config: args.config,
        },
        args.emit,
    )
}

pub fn run_place_wrapper() -> Result<()> {
    let args = CompatPlaceArgs::parse();
    run_place(
        PlaceArgs {
            input: args.input,
            output: args.output,
            arch: args.arch,
            delay: args.delay,
            constraints: args.constraints,
            mode: compat_place_mode(args.bounding, args.timing).into(),
            seed: 0xFDE_2024,
        },
        args.emit,
    )
}

pub fn run_route_wrapper() -> Result<()> {
    let args = CompatRouteArgs::parse();
    run_route(
        RouteArgs {
            input: args.input,
            output: args.output,
            arch: args.arch,
            constraints: args.constraints,
            mode: compat_route_mode(args.breadth, args.directed).into(),
        },
        args.emit,
    )
}

pub fn run_sta_wrapper() -> Result<()> {
    let args = CompatStaArgs::parse();
    run_sta(
        StaArgs {
            input: args.input,
            output: args.output,
            report: args.report,
            arch: args.arch,
            delay: args.delay,
            timing_library: args.timing_library,
        },
        args.emit,
    )
}

pub fn run_bitgen_wrapper() -> Result<()> {
    let args = CompatBitgenArgs::parse();
    run_bitgen(
        BitgenArgs {
            input: args.input,
            output: args.output,
            arch: args.arch,
            cil: args.cil,
            sidecar: None,
        },
        args.emit,
    )
}

pub fn run_nlfiner_wrapper() -> Result<()> {
    let args = CompatNlfinerArgs::parse();
    let (input, output, cell_library, config) = if args.positional.len() == 4 {
        (
            args.positional[0].clone(),
            args.positional[3].clone(),
            Some(args.positional[1].clone()),
            Some(args.positional[2].clone()),
        )
    } else {
        (
            args.input
                .ok_or_else(|| anyhow!("nlfiner requires an input path"))?,
            args.output
                .ok_or_else(|| anyhow!("nlfiner requires an output path"))?,
            args.cell_library,
            args.config,
        )
    };
    run_normalize(
        NormalizeArgs {
            input,
            output,
            cell_library,
            config,
        },
        args.emit,
    )
}

pub fn run_import_wrapper() -> Result<()> {
    let args = CompatImportArgs::parse();
    let (input, output) = if args.positional.len() == 2 {
        (args.positional[0].clone(), args.positional[1].clone())
    } else {
        (
            args.input
                .ok_or_else(|| anyhow!("import requires an input path"))?,
            args.output
                .ok_or_else(|| anyhow!("import requires an output path"))?,
        )
    };
    run_import(ImportArgs { input, output }, args.emit)
}
