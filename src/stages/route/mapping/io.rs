use crate::{
    bitgen::{DeviceCell, DeviceEndpoint},
    domain::PinRole,
};
use smallvec::SmallVec;

use super::super::types::WireInterner;
use super::WireSet;

pub(super) fn iob_source_nets(
    cell: &DeviceCell,
    endpoint: &DeviceEndpoint,
    wires: &mut WireInterner,
) -> WireSet {
    let side = cell.tile_wire_prefix();
    let index = cell.site_slot();
    if PinRole::classify_for_site(cell.site_kind_class(), &endpoint.pin).is_site_input() {
        return SmallVec::from_buf([wires.intern_composite_indexed(side, "_I", index, "")]);
    }
    WireSet::new()
}

pub(super) fn iob_sink_nets(
    cell: &DeviceCell,
    endpoint: &DeviceEndpoint,
    wires: &mut WireInterner,
) -> WireSet {
    let side = cell.tile_wire_prefix();
    let index = cell.site_slot();
    if PinRole::classify_for_site(cell.site_kind_class(), &endpoint.pin).is_site_output() {
        return SmallVec::from_buf([wires.intern_composite_indexed(side, "_O", index, "")]);
    }
    WireSet::new()
}
