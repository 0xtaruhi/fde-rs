use anyhow::Result;
use std::{path::Path, sync::Arc};

use crate::{
    bitgen::BitgenOptions,
    cil::{Cil, load_cil},
    constraints::{
        ConstraintEntry, ConstraintSet, SdcConstraintSet, SharedConstraints, load_constraint_set,
        load_sdc_constraints, merge_clock_constraints,
    },
    io::DesignWriteContext,
    ir::Design,
    resource::{Arch, load_arch},
    route::{lower_design, materialize_design_route_image},
};

pub(crate) struct PreparedBitgen {
    pub(crate) options: BitgenOptions,
}

pub(crate) fn load_constraints_or_empty(path: Option<&Path>) -> Result<SharedConstraints> {
    load_constraint_set_or_empty(path).map(|constraints| Arc::from(constraints.pins))
}

pub(crate) fn load_constraint_set_or_empty(path: Option<&Path>) -> Result<ConstraintSet> {
    match path {
        Some(path) => load_constraint_set(path),
        None => Ok(ConstraintSet::default()),
    }
}

pub(crate) fn load_timing_constraints(
    constraints_path: Option<&Path>,
    sdc_path: Option<&Path>,
) -> Result<(ConstraintSet, SdcConstraintSet)> {
    let mut constraints = load_constraint_set_or_empty(constraints_path)?;
    let mut sdc = sdc_path
        .map(load_sdc_constraints)
        .transpose()?
        .unwrap_or_default();
    merge_clock_constraints(&mut constraints.clocks, std::mem::take(&mut sdc.clocks))?;
    Ok((constraints, sdc))
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

pub(crate) fn sta_write_context(arch: Option<&Arch>) -> DesignWriteContext<'_> {
    DesignWriteContext {
        arch,
        ..DesignWriteContext::default()
    }
}
