use super::{DeviceDesign, lower_design};
use crate::{
    cil::Cil,
    constraints::ConstraintEntry,
    ir::{Design, RoutePip},
    resource::Arch,
    route_bits::{DeviceRouteImage, route_device_design},
};
use anyhow::Result;
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Clone)]
pub struct ExactRouteArtifacts {
    pub design: Design,
    pub device: DeviceDesign,
    pub route_image: DeviceRouteImage,
}

pub fn annotate_exact_route_pips(
    design: Design,
    arch: &Arch,
    arch_path: &Path,
    cil: &Cil,
    constraints: &[ConstraintEntry],
) -> Result<ExactRouteArtifacts> {
    let mut device = lower_design(design.clone(), arch, Some(cil), constraints)?;
    let route_image = route_device_design(&device, arch, arch_path, cil)?;
    let route_pips_by_net = group_route_pips_by_net(&route_image);

    let mut exact_design = design;
    for net in &mut exact_design.nets {
        net.route_pips = route_pips_by_net
            .get(&net.name)
            .cloned()
            .unwrap_or_default();
    }
    for net in &mut device.nets {
        net.route_pips = route_pips_by_net
            .get(&net.name)
            .cloned()
            .unwrap_or_default();
    }

    Ok(ExactRouteArtifacts {
        design: exact_design,
        device,
        route_image,
    })
}

fn group_route_pips_by_net(route_image: &DeviceRouteImage) -> BTreeMap<String, Vec<RoutePip>> {
    let mut grouped = BTreeMap::<String, Vec<RoutePip>>::new();
    for pip in &route_image.pips {
        grouped
            .entry(pip.net_name.clone())
            .or_default()
            .push(RoutePip {
                x: pip.x,
                y: pip.y,
                from_net: pip.from_net.clone(),
                to_net: pip.to_net.clone(),
                dir: "->".to_string(),
            });
    }
    grouped
}
