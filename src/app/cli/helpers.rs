use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::{
    bitgen::BitgenOptions,
    cil::load_cil,
    constraints::{ConstraintEntry, load_constraints},
    device::annotate_exact_route_pips,
    ir::Design,
    place::PlaceMode,
    resource::load_arch,
    route::RouteMode,
};

pub(crate) struct PreparedBitgen {
    pub(crate) options: BitgenOptions,
}

pub(crate) fn load_constraints_or_empty(path: Option<&PathBuf>) -> Result<Vec<ConstraintEntry>> {
    match path {
        Some(path) => load_constraints(path),
        None => Ok(Vec::new()),
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
    let device_design = match (arch.as_ref(), cil.as_ref(), arch_path.as_ref()) {
        (Some(arch), Some(cil), Some(arch_path)) => {
            Some(annotate_exact_route_pips(design, arch, arch_path, cil, &[])?.device)
        }
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
