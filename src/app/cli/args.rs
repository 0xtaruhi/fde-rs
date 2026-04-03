use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::{
    orchestrator::ImplementationOptions,
    pack::DEFAULT_PACK_CAPACITY,
    place::{DEFAULT_PLACE_SEED, PlaceMode},
};

#[derive(Parser)]
#[command(
    name = "fde",
    author,
    version,
    about = "Modern Rust EDA flow for FDE-style implementation",
    long_about = "Modern Rust EDA flow for Yosys-first implementation: map, pack, place, route, sta, bitgen, normalize, and full impl orchestration.",
    arg_required_else_help = true,
    subcommand_required = true,
    propagate_version = true
)]
pub(crate) struct FdeCli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    #[command(about = "Map an EDIF or IR design into the normalized mapped IR")]
    Map(MapArgs),
    #[command(about = "Pack logical cells into legal clusters")]
    Pack(PackArgs),
    #[command(about = "Place packed clusters onto the architecture grid")]
    Place(PlaceArgs),
    #[command(about = "Run the physical router and emit a routed netlist with PIPs")]
    Route(RouteArgs),
    #[command(about = "Run static timing analysis on a routed design")]
    Sta(StaArgs),
    #[command(about = "Generate a deterministic or architecture-backed bitstream artifact")]
    Bitgen(BitgenArgs),
    #[command(about = "Normalize and clean up an input netlist")]
    Normalize(NormalizeArgs),
    #[command(about = "Import an existing IR-compatible design source")]
    Import(ImportArgs),
    #[command(about = "Run the full implementation flow")]
    Impl(Box<ImplArgs>),
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum CliPlaceMode {
    Bounding,
    Timing,
}

impl From<CliPlaceMode> for PlaceMode {
    fn from(value: CliPlaceMode) -> Self {
        match value {
            CliPlaceMode::Bounding => PlaceMode::BoundingBox,
            CliPlaceMode::Timing => PlaceMode::TimingDriven,
        }
    }
}

impl From<PlaceMode> for CliPlaceMode {
    fn from(value: PlaceMode) -> Self {
        match value {
            PlaceMode::BoundingBox => CliPlaceMode::Bounding,
            PlaceMode::TimingDriven => CliPlaceMode::Timing,
        }
    }
}

#[derive(Args)]
pub(crate) struct MapArgs {
    #[arg(long, short = 'i')]
    pub(crate) input: PathBuf,
    #[arg(long, short = 'o')]
    pub(crate) output: PathBuf,
    #[arg(long, short = 'c')]
    pub(crate) cell_library: Option<PathBuf>,
    #[arg(long, default_value_t = 4)]
    pub(crate) lut_size: usize,
    #[arg(long)]
    pub(crate) verilog_output: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct PackArgs {
    #[arg(long, short = 'i')]
    pub(crate) input: PathBuf,
    #[arg(long, short = 'o')]
    pub(crate) output: PathBuf,
    #[arg(long)]
    pub(crate) family: Option<String>,
    #[arg(long, default_value_t = DEFAULT_PACK_CAPACITY)]
    pub(crate) capacity: usize,
    #[arg(long)]
    pub(crate) cell_library: Option<PathBuf>,
    #[arg(long)]
    pub(crate) dcp_library: Option<PathBuf>,
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct PlaceArgs {
    #[arg(long, short = 'i')]
    pub(crate) input: PathBuf,
    #[arg(long, short = 'o')]
    pub(crate) output: PathBuf,
    #[arg(long, short = 'a')]
    pub(crate) arch: PathBuf,
    #[arg(long, short = 'd')]
    pub(crate) delay: Option<PathBuf>,
    #[arg(long, short = 'c')]
    pub(crate) constraints: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "bounding")]
    pub(crate) mode: CliPlaceMode,
    #[arg(long, default_value_t = DEFAULT_PLACE_SEED)]
    pub(crate) seed: u64,
}

#[derive(Args)]
pub(crate) struct RouteArgs {
    #[arg(long, short = 'i')]
    pub(crate) input: PathBuf,
    #[arg(long, short = 'o')]
    pub(crate) output: PathBuf,
    #[arg(long, short = 'a')]
    pub(crate) arch: PathBuf,
    #[arg(long, short = 'c')]
    pub(crate) constraints: Option<PathBuf>,
    #[arg(long)]
    pub(crate) cil: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct StaArgs {
    #[arg(long, short = 'i')]
    pub(crate) input: PathBuf,
    #[arg(long, short = 'o')]
    pub(crate) output: PathBuf,
    #[arg(long, short = 'r')]
    pub(crate) report: PathBuf,
    #[arg(long, short = 'a')]
    pub(crate) arch: Option<PathBuf>,
    #[arg(long, short = 'd')]
    pub(crate) delay: Option<PathBuf>,
    #[arg(long)]
    pub(crate) timing_library: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct BitgenArgs {
    #[arg(long, short = 'i')]
    pub(crate) input: PathBuf,
    #[arg(long, short = 'o')]
    pub(crate) output: PathBuf,
    #[arg(long, short = 'a')]
    pub(crate) arch: Option<PathBuf>,
    #[arg(long, short = 'c')]
    pub(crate) cil: Option<PathBuf>,
    #[arg(long)]
    pub(crate) sidecar: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct NormalizeArgs {
    #[arg(long, short = 'i')]
    pub(crate) input: PathBuf,
    #[arg(long, short = 'o')]
    pub(crate) output: PathBuf,
    #[arg(long, short = 'c')]
    pub(crate) cell_library: Option<PathBuf>,
    #[arg(long, short = 'g')]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct ImportArgs {
    #[arg(long, short = 'i')]
    pub(crate) input: PathBuf,
    #[arg(long, short = 'o')]
    pub(crate) output: PathBuf,
}

#[derive(Args)]
pub(crate) struct ImplArgs {
    #[arg(long, short = 'i')]
    pub(crate) input: PathBuf,
    #[arg(long)]
    pub(crate) out_dir: PathBuf,
    #[arg(long)]
    pub(crate) resource_root: Option<PathBuf>,
    #[arg(long)]
    pub(crate) constraints: Option<PathBuf>,
    #[arg(long)]
    pub(crate) dc_cell: Option<PathBuf>,
    #[arg(long)]
    pub(crate) pack_cell: Option<PathBuf>,
    #[arg(long)]
    pub(crate) pack_lib: Option<PathBuf>,
    #[arg(long)]
    pub(crate) pack_config: Option<PathBuf>,
    #[arg(long)]
    pub(crate) arch: Option<PathBuf>,
    #[arg(long)]
    pub(crate) delay: Option<PathBuf>,
    #[arg(long)]
    pub(crate) sta_lib: Option<PathBuf>,
    #[arg(long)]
    pub(crate) cil: Option<PathBuf>,
    #[arg(long, default_value = "fdp3")]
    pub(crate) family: String,
    #[arg(long, default_value_t = 4)]
    pub(crate) lut_size: usize,
    #[arg(long, default_value_t = DEFAULT_PACK_CAPACITY)]
    pub(crate) pack_capacity: usize,
    #[arg(long, value_enum, default_value = "bounding")]
    pub(crate) place_mode: CliPlaceMode,
    #[arg(long, default_value_t = DEFAULT_PLACE_SEED)]
    pub(crate) seed: u64,
}

impl From<ImplArgs> for ImplementationOptions {
    fn from(value: ImplArgs) -> Self {
        Self {
            input: value.input,
            out_dir: value.out_dir,
            resource_root: value.resource_root,
            constraints: value.constraints,
            dc_cell: value.dc_cell,
            pack_cell: value.pack_cell,
            pack_lib: value.pack_lib,
            pack_config: value.pack_config,
            arch: value.arch,
            delay: value.delay,
            sta_lib: value.sta_lib,
            cil: value.cil,
            family: Some(value.family),
            lut_size: value.lut_size,
            pack_capacity: value.pack_capacity,
            place_mode: value.place_mode.into(),
            seed: value.seed,
        }
    }
}
