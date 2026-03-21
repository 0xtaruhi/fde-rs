use anyhow::Result;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    bitgen::BitgenOptions,
    cil::load_cil,
    constraints::{SharedConstraints, load_constraints},
    device::lower_design,
    ir::Design,
    place::PlaceMode,
    resource::load_arch,
    route::RouteMode,
};

pub(crate) struct PreparedBitgen {
    pub(crate) options: BitgenOptions,
}

pub(crate) fn load_constraints_or_empty(path: Option<&PathBuf>) -> Result<SharedConstraints> {
    match path {
        Some(path) => load_constraints(path).map(Arc::<[_]>::from),
        None => Ok(Arc::from([])),
    }
}

pub(crate) fn default_sidecar_path(output: &Path) -> PathBuf {
    output.with_extension("bit.txt")
}

pub(crate) fn prepare_bitgen(
    design: Design,
    arch_path: Option<&PathBuf>,
    cil_path: Option<&PathBuf>,
) -> Result<PreparedBitgen> {
    let arch = match arch_path {
        Some(path) => Some(load_arch(path)?),
        None => None,
    };
    let arch_name = arch.as_ref().map(|arch| arch.name.clone());
    let cil = match cil_path {
        Some(path) => Some(load_cil(path)?),
        None => None,
    };
    let device_design = match (arch.as_ref(), cil.as_ref()) {
        (Some(arch), Some(cil)) => Some(lower_design(design, arch, Some(cil), &[])?),
        _ => None,
    };
    Ok(PreparedBitgen {
        options: BitgenOptions {
            arch_name,
            arch_path: arch_path.cloned(),
            cil_path: cil_path.cloned(),
            cil,
            device_design,
        },
    })
}

pub(crate) fn compat_place_mode(bounding: bool, timing: bool) -> PlaceMode {
    if bounding && !timing {
        PlaceMode::BoundingBox
    } else {
        PlaceMode::TimingDriven
    }
}

pub(crate) fn compat_route_mode(breadth: bool, directed: bool) -> RouteMode {
    if breadth {
        RouteMode::BreadthFirst
    } else if directed {
        RouteMode::Directed
    } else {
        RouteMode::TimingDriven
    }
}
