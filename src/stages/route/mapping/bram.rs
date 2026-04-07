use super::shared::WireSet;
use crate::{DeviceCell, DeviceEndpoint, domain::block_ram_route_target};

use super::super::types::WireInterner;

pub(super) fn bram_source_nets(
    _cell: &DeviceCell,
    endpoint: &DeviceEndpoint,
    wires: &mut WireInterner,
) -> WireSet {
    let mut set = WireSet::new();
    if let Some(target) = block_ram_route_target(&endpoint.pin) {
        set.push(wires.intern(&target.wire_name));
    }
    set
}

pub(super) fn bram_sink_nets(
    _cell: &DeviceCell,
    endpoint: &DeviceEndpoint,
    wires: &mut WireInterner,
) -> WireSet {
    let mut set = WireSet::new();
    if let Some(target) = block_ram_route_target(&endpoint.pin) {
        set.push(wires.intern(&target.wire_name));
    }
    set
}
