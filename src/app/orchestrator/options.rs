use crate::{pack::DEFAULT_PACK_CAPACITY, place::PlaceMode, route::RouteMode};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationOptions {
    pub input: PathBuf,
    pub out_dir: PathBuf,
    pub resource_root: Option<PathBuf>,
    pub constraints: Option<PathBuf>,
    pub dc_cell: Option<PathBuf>,
    pub pack_cell: Option<PathBuf>,
    pub pack_lib: Option<PathBuf>,
    pub pack_config: Option<PathBuf>,
    pub arch: Option<PathBuf>,
    pub delay: Option<PathBuf>,
    pub sta_lib: Option<PathBuf>,
    pub cil: Option<PathBuf>,
    pub family: Option<String>,
    pub lut_size: usize,
    pub pack_capacity: usize,
    pub place_mode: PlaceMode,
    pub route_mode: RouteMode,
    pub seed: u64,
}

impl Default for ImplementationOptions {
    fn default() -> Self {
        Self {
            input: PathBuf::new(),
            out_dir: PathBuf::from("build/fde-run"),
            resource_root: None,
            constraints: None,
            dc_cell: None,
            pack_cell: None,
            pack_lib: None,
            pack_config: None,
            arch: None,
            delay: None,
            sta_lib: None,
            cil: None,
            family: Some("fdp3".to_string()),
            lut_size: 4,
            pack_capacity: DEFAULT_PACK_CAPACITY,
            place_mode: PlaceMode::TimingDriven,
            route_mode: RouteMode::TimingDriven,
            seed: 0xFDE_2024,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedResources {
    pub(crate) dc_cell: Option<PathBuf>,
    pub(crate) pack_cell: Option<PathBuf>,
    pub(crate) pack_lib: Option<PathBuf>,
    pub(crate) pack_config: Option<PathBuf>,
    pub(crate) arch: PathBuf,
    pub(crate) delay: Option<PathBuf>,
    pub(crate) sta_lib: Option<PathBuf>,
    pub(crate) cil: Option<PathBuf>,
}
