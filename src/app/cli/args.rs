use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::{orchestrator::ImplementationOptions, place::PlaceMode, route::RouteMode};

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
    #[command(about = "Route the placed design on the coarse routing grid")]
    Route(RouteArgs),
    #[command(about = "Run static timing analysis on a routed design")]
    Sta(StaArgs),
    #[command(about = "Generate a deterministic or architecture-backed bitstream artifact")]
    Bitgen(BitgenArgs),
    #[command(
        about = "Normalize and clean up an input netlist",
        visible_alias = "nlfiner"
    )]
    Normalize(NormalizeArgs),
    #[command(about = "Import an existing IR-compatible design source")]
    Import(ImportArgs),
    #[command(
        about = "Run the full implementation flow",
        visible_alias = "implement"
    )]
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

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum CliRouteMode {
    Breadth,
    Directed,
    Timing,
}

impl From<CliRouteMode> for RouteMode {
    fn from(value: CliRouteMode) -> Self {
        match value {
            CliRouteMode::Breadth => RouteMode::BreadthFirst,
            CliRouteMode::Directed => RouteMode::Directed,
            CliRouteMode::Timing => RouteMode::TimingDriven,
        }
    }
}

impl From<RouteMode> for CliRouteMode {
    fn from(value: RouteMode) -> Self {
        match value {
            RouteMode::BreadthFirst => CliRouteMode::Breadth,
            RouteMode::Directed => CliRouteMode::Directed,
            RouteMode::TimingDriven => CliRouteMode::Timing,
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
    #[arg(long, default_value_t = 4)]
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
    #[arg(long, value_enum, default_value = "timing")]
    pub(crate) mode: CliPlaceMode,
    #[arg(long, default_value_t = 0xFDE_2024)]
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
    #[arg(long, value_enum, default_value = "timing")]
    pub(crate) mode: CliRouteMode,
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
    #[arg(long, default_value_t = 4)]
    pub(crate) pack_capacity: usize,
    #[arg(long, value_enum, default_value = "timing")]
    pub(crate) place_mode: CliPlaceMode,
    #[arg(long, value_enum, default_value = "timing")]
    pub(crate) route_mode: CliRouteMode,
    #[arg(long, default_value_t = 0xFDE_2024)]
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
            route_mode: value.route_mode.into(),
            seed: value.seed,
        }
    }
}

#[derive(Parser)]
#[command(name = "map", arg_required_else_help = true)]
pub(crate) struct CompatMapArgs {
    #[arg(short = 'i')]
    pub(crate) input: PathBuf,
    #[arg(short = 'o')]
    pub(crate) output: PathBuf,
    #[arg(short = 'c')]
    pub(crate) cell_library: Option<PathBuf>,
    #[arg(short = 'v')]
    pub(crate) verilog_output: Option<PathBuf>,
    #[arg(short = 'k', default_value_t = 4)]
    pub(crate) lut_size: usize,
    #[arg(short = 'y')]
    pub(crate) _yosys_edif: bool,
    #[arg(short = 'e')]
    pub(crate) emit: bool,
}

#[derive(Parser)]
#[command(name = "pack", arg_required_else_help = true)]
pub(crate) struct CompatPackArgs {
    #[arg(short = 'c')]
    pub(crate) family: Option<String>,
    #[arg(short = 'n')]
    pub(crate) input: PathBuf,
    #[arg(short = 'l')]
    pub(crate) cell_library: Option<PathBuf>,
    #[arg(short = 'r')]
    pub(crate) dcp_library: Option<PathBuf>,
    #[arg(short = 'o')]
    pub(crate) output: PathBuf,
    #[arg(short = 'g')]
    pub(crate) config: Option<PathBuf>,
    #[arg(long, default_value_t = 4)]
    pub(crate) capacity: usize,
    #[arg(short = 'e')]
    pub(crate) emit: bool,
}

#[derive(Parser)]
#[command(name = "place", arg_required_else_help = true)]
pub(crate) struct CompatPlaceArgs {
    #[arg(short = 'a')]
    pub(crate) arch: PathBuf,
    #[arg(short = 'i')]
    pub(crate) input: PathBuf,
    #[arg(short = 'o')]
    pub(crate) output: PathBuf,
    #[arg(short = 'd')]
    pub(crate) delay: Option<PathBuf>,
    #[arg(short = 'c')]
    pub(crate) constraints: Option<PathBuf>,
    #[arg(short = 'b')]
    pub(crate) bounding: bool,
    #[arg(short = 't')]
    pub(crate) timing: bool,
    #[arg(short = 'u')]
    pub(crate) _unused: Option<String>,
    #[arg(short = 'e')]
    pub(crate) emit: bool,
}

#[derive(Parser)]
#[command(name = "route", arg_required_else_help = true)]
pub(crate) struct CompatRouteArgs {
    #[arg(short = 'a')]
    pub(crate) arch: PathBuf,
    #[arg(short = 'n')]
    pub(crate) input: PathBuf,
    #[arg(short = 'o')]
    pub(crate) output: PathBuf,
    #[arg(short = 'c')]
    pub(crate) constraints: Option<PathBuf>,
    #[arg(short = 'b')]
    pub(crate) breadth: bool,
    #[arg(short = 'd')]
    pub(crate) directed: bool,
    #[arg(short = 't')]
    pub(crate) timing: bool,
    #[arg(short = 'i')]
    pub(crate) _unused: Option<PathBuf>,
    #[arg(short = 'v')]
    pub(crate) _verbose: bool,
    #[arg(short = 'e')]
    pub(crate) emit: bool,
}

#[derive(Parser)]
#[command(name = "sta", arg_required_else_help = true)]
pub(crate) struct CompatStaArgs {
    #[arg(short = 'a')]
    pub(crate) arch: Option<PathBuf>,
    #[arg(short = 'i')]
    pub(crate) input: PathBuf,
    #[arg(short = 'o')]
    pub(crate) output: PathBuf,
    #[arg(short = 'l')]
    pub(crate) timing_library: Option<PathBuf>,
    #[arg(short = 'r')]
    pub(crate) report: PathBuf,
    #[arg(short = 'd')]
    pub(crate) delay: Option<PathBuf>,
    #[arg(short = 'c')]
    pub(crate) _corner: Option<String>,
    #[arg(short = 'n')]
    pub(crate) _name: Option<String>,
    #[arg(short = 's')]
    pub(crate) _sdc: Option<PathBuf>,
    #[arg(short = 'e')]
    pub(crate) emit: bool,
}

#[derive(Parser)]
#[command(name = "bitgen", arg_required_else_help = true)]
pub(crate) struct CompatBitgenArgs {
    #[arg(short = 'a')]
    pub(crate) arch: Option<PathBuf>,
    #[arg(short = 'c')]
    pub(crate) cil: Option<PathBuf>,
    #[arg(short = 'n')]
    pub(crate) input: PathBuf,
    #[arg(short = 'b')]
    pub(crate) output: PathBuf,
    #[arg(short = 'p')]
    pub(crate) _part: Option<String>,
    #[arg(short = 'f')]
    pub(crate) _frames: Option<PathBuf>,
    #[arg(short = 's')]
    pub(crate) _seed: Option<String>,
    #[arg(short = 'e')]
    pub(crate) emit: bool,
}

#[derive(Parser)]
#[command(name = "nlfiner", arg_required_else_help = true)]
pub(crate) struct CompatNlfinerArgs {
    #[arg(short = 'i')]
    pub(crate) input: Option<PathBuf>,
    #[arg(short = 'o')]
    pub(crate) output: Option<PathBuf>,
    #[arg(short = 'c')]
    pub(crate) cell_library: Option<PathBuf>,
    #[arg(short = 'g')]
    pub(crate) config: Option<PathBuf>,
    #[arg()]
    pub(crate) positional: Vec<PathBuf>,
    #[arg(short = 'e')]
    pub(crate) emit: bool,
}

#[derive(Parser)]
#[command(name = "import", arg_required_else_help = true)]
pub(crate) struct CompatImportArgs {
    #[arg(short = 'i')]
    pub(crate) input: Option<PathBuf>,
    #[arg(short = 'o')]
    pub(crate) output: Option<PathBuf>,
    #[arg()]
    pub(crate) positional: Vec<PathBuf>,
    #[arg(short = 'e')]
    pub(crate) emit: bool,
}
