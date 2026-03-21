mod cells;
mod nets;
mod ports;

use super::types::{DeviceCell, DeviceDesign, DeviceEndpoint, DeviceNet, DevicePort};
use crate::{
    cil::Cil,
    constraints::{
        ConstraintEntry, apply_constraints, ensure_cluster_positions, ensure_port_positions,
    },
    domain::SiteKind,
    ir::{CellId, Design, DesignIndex, PortId},
    resource::{Arch, PadSiteKind},
};
use anyhow::Result;

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
    index: DesignIndex<'a>,
    device: DeviceDesign,
    device_ports: Vec<Option<usize>>,
    original_cells: Vec<Option<usize>>,
    io_cells: Vec<Option<usize>>,
    gclk_cells: Vec<Option<usize>>,
}

#[derive(Clone)]
struct ResolvedPortSite {
    pin_name: String,
    site_kind: SiteKind,
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
            index: design.index(),
            device: DeviceDesign {
                name: design.name.clone(),
                device: arch.name.clone(),
                ..DeviceDesign::default()
            },
            device_ports: vec![None; design.ports.len()],
            original_cells: vec![None; design.cells.len()],
            io_cells: vec![None; design.ports.len()],
            gclk_cells: vec![None; design.ports.len()],
        }
    }

    fn into_device(self) -> DeviceDesign {
        self.device
    }

    fn push_device_port(&mut self, port_id: PortId, port: DevicePort) -> usize {
        let slot = self.device.ports.len();
        self.device.ports.push(port);
        self.device_ports[port_id.index()] = Some(slot);
        slot
    }

    fn push_original_cell(&mut self, cell_id: CellId, cell: DeviceCell) -> usize {
        let slot = self.device.cells.len();
        self.device.cells.push(cell);
        self.original_cells[cell_id.index()] = Some(slot);
        slot
    }

    fn push_synthetic_cell(&mut self, cell: DeviceCell) -> usize {
        let slot = self.device.cells.len();
        self.device.cells.push(cell);
        slot
    }

    fn bind_io_cell(&mut self, port_id: PortId, cell: DeviceCell) -> usize {
        let slot = self.push_synthetic_cell(cell);
        self.io_cells[port_id.index()] = Some(slot);
        slot
    }

    fn bind_gclk_cell(&mut self, port_id: PortId, cell: DeviceCell) -> usize {
        let slot = self.push_synthetic_cell(cell);
        self.gclk_cells[port_id.index()] = Some(slot);
        slot
    }

    fn device_port(&self, port_id: PortId) -> Option<&DevicePort> {
        self.device_ports
            .get(port_id.index())
            .copied()
            .flatten()
            .and_then(|slot| self.device.ports.get(slot))
    }

    fn original_cell(&self, cell_id: CellId) -> Option<&DeviceCell> {
        self.original_cells
            .get(cell_id.index())
            .copied()
            .flatten()
            .and_then(|slot| self.device.cells.get(slot))
    }

    fn io_cell(&self, port_id: PortId) -> Option<&DeviceCell> {
        self.io_cells
            .get(port_id.index())
            .copied()
            .flatten()
            .and_then(|slot| self.device.cells.get(slot))
    }

    fn gclk_cell(&self, port_id: PortId) -> Option<&DeviceCell> {
        self.gclk_cells
            .get(port_id.index())
            .copied()
            .flatten()
            .and_then(|slot| self.device.cells.get(slot))
    }

    fn finish_notes(&mut self) {
        self.device.notes.push(
            "Device lowering materializes synthetic IOB/GCLK sites and BEL anchors for future Rust bitgen.".to_string(),
        );
    }
}
