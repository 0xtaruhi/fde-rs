use crate::{
    bitgen::{DeviceCell, DeviceEndpoint},
    domain::{
        PinRole, SliceControlWireKind, slice_control_wire_name, slice_lut_input_wire_prefix,
        slice_lut_output_wire_name, slice_register_data_wire_name, slice_register_output_wire_name,
    },
};
use smallvec::SmallVec;

use super::super::types::WireInterner;
use super::{
    WireSet,
    shared::{bel_slot, pin_map_indices},
};

pub(super) fn slice_source_nets(
    cell: &DeviceCell,
    endpoint: &DeviceEndpoint,
    wires: &mut WireInterner,
) -> WireSet {
    let slot = bel_slot(&cell.bel).unwrap_or(0);
    let pin_role = PinRole::classify_for_primitive(cell.primitive_kind(), &endpoint.pin);
    if pin_role == PinRole::RegisterOutput {
        let wire = slice_register_output_wire_name(&cell.site_name, slot);
        return SmallVec::from_vec(vec![wires.intern(&wire)]);
    }
    if pin_role == PinRole::LutOutput {
        let wire = slice_lut_output_wire_name(&cell.site_name, slot);
        return SmallVec::from_vec(vec![wires.intern(&wire)]);
    }
    WireSet::new()
}

pub(super) fn slice_sink_nets(
    driver_cell: Option<&DeviceCell>,
    cell: &DeviceCell,
    endpoint: &DeviceEndpoint,
    wires: &mut WireInterner,
) -> WireSet {
    let slot = bel_slot(&cell.bel).unwrap_or(0);
    let pin_role = PinRole::classify_for_primitive(cell.primitive_kind(), &endpoint.pin);
    if let Some(index) = pin_role.lut_input_index() {
        let lut_prefix = slice_lut_input_wire_prefix(&cell.site_name, slot);
        let sink_wires = pin_map_indices(cell, index)
            .into_iter()
            .map(|physical_index| wires.intern_indexed(&lut_prefix, physical_index + 1))
            .collect::<Vec<_>>();
        return SmallVec::from_vec(sink_wires);
    }
    if pin_role == PinRole::RegisterClock {
        let wire = slice_control_wire_name(&cell.site_name, SliceControlWireKind::Clock);
        return SmallVec::from_vec(vec![wires.intern(&wire)]);
    }
    if pin_role == PinRole::RegisterClockEnable {
        let wire = slice_control_wire_name(&cell.site_name, SliceControlWireKind::ClockEnable);
        return SmallVec::from_vec(vec![wires.intern(&wire)]);
    }
    if pin_role == PinRole::RegisterSetReset {
        let wire = slice_control_wire_name(&cell.site_name, SliceControlWireKind::SetReset);
        return SmallVec::from_vec(vec![wires.intern(&wire)]);
    }
    if pin_role == PinRole::RegisterData && !register_data_uses_local_lut(driver_cell, cell, slot) {
        let wire = slice_register_data_wire_name(&cell.site_name, slot);
        return SmallVec::from_vec(vec![wires.intern(&wire)]);
    }
    WireSet::new()
}

pub(crate) fn should_skip_unmapped_sink(
    driver_cell: Option<&DeviceCell>,
    cell: &DeviceCell,
    endpoint: &DeviceEndpoint,
) -> bool {
    cell.site_kind_class().is_logic_slice()
        && PinRole::classify_for_primitive(cell.primitive_kind(), &endpoint.pin)
            == PinRole::RegisterData
        && register_data_uses_local_lut(driver_cell, cell, bel_slot(&cell.bel).unwrap_or(0))
}

pub(crate) fn sink_requires_all_wires(cell: &DeviceCell, endpoint: &DeviceEndpoint) -> bool {
    PinRole::classify_for_primitive(cell.primitive_kind(), &endpoint.pin)
        .lut_input_index()
        .is_some_and(|logical_index| pin_map_indices(cell, logical_index).len() > 1)
}

fn register_data_uses_local_lut(
    driver_cell: Option<&DeviceCell>,
    sink_cell: &DeviceCell,
    slot: usize,
) -> bool {
    let Some(driver_cell) = driver_cell else {
        return false;
    };
    driver_cell.site_kind_class().is_logic_slice()
        && driver_cell.primitive_kind().is_lut()
        && driver_cell.tile_name == sink_cell.tile_name
        && driver_cell.site_name == sink_cell.site_name
        && driver_cell.z == sink_cell.z
        && bel_slot(&driver_cell.bel).unwrap_or(usize::MAX).min(1) == slot.min(1)
}
