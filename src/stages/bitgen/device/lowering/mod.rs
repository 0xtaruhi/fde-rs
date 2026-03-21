mod cells;
mod nets;
mod ports;

use super::types::{DeviceCell, DeviceDesign, DeviceEndpoint, DeviceNet, DevicePort};
use crate::{
    cil::Cil,
    constraints::{
        ConstraintEntry, apply_constraints, ensure_cluster_positions, ensure_port_positions,
    },
    ir::Design,
    resource::{Arch, PadSiteKind},
};
use anyhow::Result;
use std::collections::BTreeMap;

pub fn lower_design(
    mut design: Design,
    arch: &Arch,
    cil: Option<&Cil>,
    constraints: &[ConstraintEntry],
) -> Result<DeviceDesign> {
    apply_constraints(&mut design, arch, constraints);
    ensure_port_positions(&mut design, arch);
    if !design.clusters.is_empty() {
        ensure_cluster_positions(&design)?;
    }

    let mut lowering = DeviceLowering::new(&design, arch, cil);
    lowering.materialize_ports();
    lowering.materialize_cells();
    lowering.materialize_nets();
    lowering.finish_notes();

    Ok(lowering.into_device())
}

struct DeviceLowering<'a> {
    design: &'a Design,
    arch: &'a Arch,
    cil: Option<&'a Cil>,
    device: DeviceDesign,
    port_to_io: BTreeMap<String, String>,
    port_to_gclk: BTreeMap<String, String>,
}

#[derive(Clone)]
struct ResolvedPortSite {
    pin_name: String,
    site_kind: String,
    site_name: String,
    tile_name: String,
    tile_type: String,
    x: usize,
    y: usize,
    z: usize,
    pad_kind: PadSiteKind,
}

impl<'a> DeviceLowering<'a> {
    fn new(design: &'a Design, arch: &'a Arch, cil: Option<&'a Cil>) -> Self {
        Self {
            design,
            arch,
            cil,
            device: DeviceDesign {
                name: design.name.clone(),
                device: arch.name.clone(),
                ..DeviceDesign::default()
            },
            port_to_io: BTreeMap::new(),
            port_to_gclk: BTreeMap::new(),
        }
    }

    fn into_device(self) -> DeviceDesign {
        self.device
    }

    fn finish_notes(&mut self) {
        self.device.notes.push(
            "Device lowering materializes synthetic IOB/GCLK sites and BEL anchors for future Rust bitgen.".to_string(),
        );
        if self.design.clusters.iter().any(|cluster| {
            cluster.members.len() > 1
                && self
                    .arch
                    .tile_at(cluster.x.unwrap_or(0), cluster.y.unwrap_or(0))
                    .is_some()
        }) {
            self.device.notes.push(
                "Cluster placement is still one-site-per-coordinate; multi-site tile packing remains a follow-up.".to_string(),
            );
        }
    }
}
