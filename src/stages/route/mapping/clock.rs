use crate::{
    bitgen::{DeviceCell, DeviceEndpoint},
    domain::PinRole,
};
use smallvec::SmallVec;

use super::super::types::WireInterner;
use super::WireSet;

pub(super) fn gclkiob_source_nets(
    cell: &DeviceCell,
    endpoint: &DeviceEndpoint,
    wires: &mut WireInterner,
) -> WireSet {
    if PinRole::classify_for_site(cell.site_kind_class(), &endpoint.pin).is_global_clock_output() {
        return SmallVec::from_buf([wires.intern_composite_indexed(
            cell.tile_wire_prefix(),
            "_CLKPAD",
            cell.site_slot(),
            "",
        )]);
    }
    WireSet::new()
}

pub(super) fn gclk_source_nets(
    cell: &DeviceCell,
    endpoint: &DeviceEndpoint,
    wires: &mut WireInterner,
) -> WireSet {
    if PinRole::classify_for_site(cell.site_kind_class(), &endpoint.pin).is_global_clock_output() {
        return SmallVec::from_buf([wires.intern_composite_indexed(
            cell.tile_wire_prefix(),
            "_GCLK",
            cell.z,
            "_PW",
        )]);
    }
    WireSet::new()
}

pub(super) fn gclk_sink_nets(
    cell: &DeviceCell,
    endpoint: &DeviceEndpoint,
    wires: &mut WireInterner,
) -> WireSet {
    if PinRole::classify_for_site(cell.site_kind_class(), &endpoint.pin).is_global_clock_input() {
        return SmallVec::from_buf([wires.intern_composite_indexed(
            cell.tile_wire_prefix(),
            "_GCLKBUF",
            cell.z,
            "_IN",
        )]);
    }
    WireSet::new()
}
