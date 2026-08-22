use anyhow::Result;
use std::{path::Path, sync::Arc};

use crate::{
    bitgen::BitgenOptions,
    cil::{Cil, load_cil},
    constraints::{ConstraintEntry, SharedConstraints, load_constraints},
    io::DesignWriteContext,
    ir::Design,
    resource::{Arch, load_arch},
    route::{lower_design, materialize_design_route_image},
};

pub(crate) struct PreparedBitgen {
    pub(crate) options: BitgenOptions,
}

pub(crate) fn load_constraints_or_empty(path: Option<&Path>) -> Result<SharedConstraints> {
    match path {
        Some(path) => load_constraints(path).map(Arc::<[_]>::from),
        None => Ok(Arc::from([])),
    }
}

pub(crate) fn prepare_route_device_design(
    design: &Design,
    arch: &Arch,
    cil: Option<&Cil>,
    constraints: &[ConstraintEntry],
) -> Result<Option<crate::DeviceDesign>> {
    cil.map(|cil| lower_design(design.clone(), arch, Some(cil), constraints))
        .transpose()
}

pub(crate) fn prepare_bitgen(
    design: &Design,
    arch_path: Option<&Path>,
    cil_path: Option<&Path>,
) -> Result<PreparedBitgen> {
    let arch = match arch_path {
        Some(path) => Some(load_arch(path)?),
        None => None,
    };
    let cil = match cil_path {
        Some(path) => Some(load_cil(path)?),
        None => None,
    };
    let arch_name = arch.as_ref().map(|arch| arch.name.clone());
    let device_design = match (arch.as_ref(), cil.as_ref()) {
        (Some(arch), Some(cil)) => prepare_route_device_design(design, arch, Some(cil), &[])?,
        _ => None,
    };
    let route_image = match (arch.as_ref(), arch_path, cil.as_ref()) {
        (Some(arch), Some(arch_path), Some(cil)) => {
            materialize_design_route_image(design, arch, arch_path, cil)?
        }
        _ => None,
    };

    Ok(PreparedBitgen {
        options: BitgenOptions {
            arch_name,
            arch_path: arch_path.map(Path::to_path_buf),
            cil_path: cil_path.map(Path::to_path_buf),
            cil,
            device_design,
            route_image,
        },
    })
}

pub(crate) fn place_write_context<'a>(
    arch: &'a Arch,
    constraints: &'a [ConstraintEntry],
) -> DesignWriteContext<'a> {
    DesignWriteContext {
        arch: Some(arch),
        constraints,
        ..DesignWriteContext::default()
    }
}

pub(crate) fn route_write_context<'a>(
    arch: &'a Arch,
    cil: Option<&'a Cil>,
    constraints: &'a [ConstraintEntry],
    cil_path: Option<&'a Path>,
) -> DesignWriteContext<'a> {
    DesignWriteContext {
        arch: Some(arch),
        cil,
        constraints,
        cil_path,
    }
}

pub(crate) fn sta_write_context<'a>(arch: Option<&'a Arch>) -> DesignWriteContext<'a> {
    DesignWriteContext {
        arch,
        ..DesignWriteContext::default()
    }
}
