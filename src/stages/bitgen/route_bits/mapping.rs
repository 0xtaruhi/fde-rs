use crate::{
    device::{DeviceCell, DeviceEndpoint, DeviceNet},
    domain::{PinRole, SiteKind},
};
use smallvec::SmallVec;

use super::types::{WireId, WireInterner};

type WireSet = SmallVec<[WireId; 1]>;

pub(crate) fn should_route_device_net(net: &DeviceNet) -> bool {
    if net.origin_kind().is_synthetic_pad() {
        return false;
    }
    net.driver.as_ref().is_some_and(DeviceEndpoint::is_cell)
        && net.sinks.iter().any(DeviceEndpoint::is_cell)
}

pub(crate) fn endpoint_source_nets(
    cell: &DeviceCell,
    endpoint: &DeviceEndpoint,
    wires: &mut WireInterner,
) -> WireSet {
    match cell.site_kind_class() {
        SiteKind::LogicSlice => slice_source_nets(cell, endpoint, wires),
        SiteKind::Iob => iob_source_nets(cell, endpoint, wires),
        SiteKind::GclkIob => gclkiob_source_nets(cell, endpoint, wires),
        SiteKind::Gclk => gclk_source_nets(cell, endpoint, wires),
        SiteKind::Const | SiteKind::Unplaced | SiteKind::Unknown => WireSet::new(),
    }
}

pub(crate) fn endpoint_sink_nets(
    cell: &DeviceCell,
    endpoint: &DeviceEndpoint,
    wires: &mut WireInterner,
) -> WireSet {
    match cell.site_kind_class() {
        SiteKind::LogicSlice => slice_sink_nets(cell, endpoint, wires),
        SiteKind::Iob => iob_sink_nets(cell, endpoint, wires),
        SiteKind::Gclk => gclk_sink_nets(cell, endpoint, wires),
        SiteKind::GclkIob | SiteKind::Const | SiteKind::Unplaced | SiteKind::Unknown => {
            WireSet::new()
        }
    }
}

fn slice_source_nets(
    cell: &DeviceCell,
    endpoint: &DeviceEndpoint,
    wires: &mut WireInterner,
) -> WireSet {
    let slot = bel_slot(&cell.bel).unwrap_or(0);
    let prefix = slice_site_prefix(cell);
    let pin_role = PinRole::classify_for_primitive(cell.primitive_kind(), &endpoint.pin);
    if pin_role == PinRole::RegisterOutput {
        let wire = if slot == 0 {
            format!("{prefix}_XQ")
        } else {
            format!("{prefix}_YQ")
        };
        return SmallVec::from_vec(vec![wires.intern(&wire)]);
    }
    if pin_role == PinRole::LutOutput {
        let wire = if slot == 0 {
            format!("{prefix}_X")
        } else {
            format!("{prefix}_Y")
        };
        return SmallVec::from_vec(vec![wires.intern(&wire)]);
    }
    WireSet::new()
}

fn slice_sink_nets(
    cell: &DeviceCell,
    endpoint: &DeviceEndpoint,
    wires: &mut WireInterner,
) -> WireSet {
    let slot = bel_slot(&cell.bel).unwrap_or(0);
    let prefix = slice_site_prefix(cell);
    let pin_role = PinRole::classify_for_primitive(cell.primitive_kind(), &endpoint.pin);
    if let Some(index) = pin_role.lut_input_index() {
        let lut_prefix = if slot == 0 {
            format!("{prefix}_F_B")
        } else {
            format!("{prefix}_G_B")
        };
        return SmallVec::from_vec(vec![wires.intern_indexed(&lut_prefix, index + 1)]);
    }
    if pin_role == PinRole::RegisterClock {
        return SmallVec::from_vec(vec![wires.intern(&format!("{prefix}_CLK_B"))]);
    }
    if pin_role == PinRole::RegisterClockEnable {
        return SmallVec::from_vec(vec![wires.intern(&format!("{prefix}_CE_B"))]);
    }
    if pin_role == PinRole::RegisterSetReset {
        return SmallVec::from_vec(vec![wires.intern(&format!("{prefix}_SR_B"))]);
    }
    WireSet::new()
}

fn iob_source_nets(
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

fn iob_sink_nets(
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

fn gclkiob_source_nets(
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

fn gclk_source_nets(
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

fn gclk_sink_nets(
    cell: &DeviceCell,
    endpoint: &DeviceEndpoint,
    wires: &mut WireInterner,
) -> WireSet {
    if PinRole::classify_for_site(cell.site_kind_class(), &endpoint.pin).is_global_clock_input() {
        return SmallVec::from_buf([wires.intern_composite_indexed(
            cell.tile_wire_prefix(),
            "_GCLK",
            cell.z,
            "",
        )]);
    }
    WireSet::new()
}

pub(crate) fn should_skip_unmapped_sink(cell: &DeviceCell, endpoint: &DeviceEndpoint) -> bool {
    cell.site_kind_class().is_logic_slice()
        && PinRole::classify_for_primitive(cell.primitive_kind(), &endpoint.pin)
            == PinRole::RegisterData
}

fn bel_slot(bel: &str) -> Option<usize> {
    bel.chars()
        .rev()
        .find(|ch| ch.is_ascii_digit())
        .and_then(|ch| ch.to_digit(10))
        .map(|digit| digit as usize)
}

fn slice_site_prefix(cell: &DeviceCell) -> &str {
    if cell.site_name.starts_with('S') && cell.site_name[1..].chars().all(|ch| ch.is_ascii_digit())
    {
        cell.site_name.as_str()
    } else {
        "S0"
    }
}
