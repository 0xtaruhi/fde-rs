#[path = "stages/analysis/mod.rs"]
pub(crate) mod analysis;
#[path = "stages/bitgen/mod.rs"]
pub mod bitgen;
#[path = "infra/cil/mod.rs"]
pub mod cil;
#[path = "app/cli/mod.rs"]
pub mod cli;
#[path = "stages/bitgen/config_image/mod.rs"]
pub mod config_image;
#[path = "infra/constraints/mod.rs"]
pub mod constraints;
#[path = "stages/bitgen/device/mod.rs"]
pub mod device;
#[path = "core/domain/mod.rs"]
pub mod domain;
#[path = "infra/edif/mod.rs"]
pub mod edif;
#[path = "stages/bitgen/frame_bitstream/mod.rs"]
pub mod frame_bitstream;
#[path = "stages/import/mod.rs"]
pub mod import;
#[path = "infra/io/mod.rs"]
pub mod io;
#[path = "core/ir/mod.rs"]
pub mod ir;
#[path = "stages/map/mod.rs"]
pub mod map;
#[path = "stages/normalize/mod.rs"]
pub mod normalize;
#[path = "app/orchestrator/mod.rs"]
pub mod orchestrator;
#[path = "stages/pack/mod.rs"]
pub mod pack;
#[path = "stages/place/mod.rs"]
pub mod place;
#[path = "app/report/mod.rs"]
pub mod report;
#[path = "infra/resource/mod.rs"]
pub mod resource;
#[path = "stages/route/mod.rs"]
pub mod route;
#[path = "stages/bitgen/route_bits/mod.rs"]
pub mod route_bits;
#[path = "stages/sta/mod.rs"]
pub mod sta;

pub use bitgen::{BitgenOptions, run as run_bitgen};
pub use cil::{Cil, load_cil};
pub use config_image::{
    AppliedSiteConfig, ConfigImage, TileBitAssignment, TileConfigImage, build_config_image,
};
pub use constraints::{ConstraintEntry, load_constraints};
pub use device::{DeviceCell, DeviceDesign, DeviceEndpoint, DeviceNet, DevicePort, lower_design};
pub use domain::{ConstantKind, EndpointKind, NetOrigin, PinRole, PrimitiveKind, SiteKind};
pub use frame_bitstream::{SerializedTextBitstream, serialize_text_bitstream};
pub use import::{ImportOptions, run_path as run_import};
pub use ir::{
    BitstreamImage, Design, Placement, PlacementSite, RoutePip, RouteSegment, TimingGraph,
    TimingSummary,
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
