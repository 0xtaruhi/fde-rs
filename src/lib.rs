mod app;
pub mod config_image;
mod core;
pub mod device;
pub mod frame_bitstream;
mod infra;
pub mod route_bits;
mod stages;

pub use app::{cli, orchestrator, report};
pub use core::{domain, ir};
pub use infra::{cil, constraints, edif, io, resource};
pub(crate) use stages::analysis;
pub use stages::{bitgen, import, map, normalize, pack, place, route, sta};

pub use bitgen::{BitgenOptions, run as run_bitgen};
pub use cil::{Cil, load_cil};
pub use config_image::{
    AppliedSiteConfig, ConfigImage, TileBitAssignment, TileConfigImage, build_config_image,
};
pub use constraints::{ConstraintEntry, load_constraints};
pub use device::{DeviceCell, DeviceDesign, DeviceEndpoint, DeviceNet, DevicePort, lower_design};
pub use domain::{
    CellKind, ClusterKind, ConstantKind, EndpointKind, NetOrigin, PinRole, PrimitiveKind, SiteKind,
    TimingPathCategory,
};
pub use frame_bitstream::{SerializedTextBitstream, serialize_text_bitstream};
pub use import::{ImportOptions, run_path as run_import};
pub use ir::{
    BitstreamImage, Design, Placement, PlacementSite, RouteSegment, TimingGraph, TimingSummary,
};
pub use map::{MapOptions, load_input as load_map_input, run as run_map};
pub use normalize::{NormalizeOptions, run as run_normalize};
pub use orchestrator::{ImplementationOptions, run as run_implementation};
pub use pack::{PackOptions, run as run_pack};
pub use place::{PlaceMode, PlaceOptions, run as run_place};
pub use report::{ImplementationReport, StageOutput, StageReport};
pub use resource::{Arch, DelayModel, ResourceBundle, load_arch, load_delay_model};
pub use route::{RouteMode, RouteOptions, run as run_route};
pub use route_bits::{DeviceRouteImage, DeviceRoutePip, RouteBit, route_device_design};
pub use sta::{StaArtifact, StaError, StaOptions, run as run_sta};
