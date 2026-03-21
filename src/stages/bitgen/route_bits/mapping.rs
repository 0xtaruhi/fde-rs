use crate::{
    device::{DeviceCell, DeviceEndpoint, DeviceNet},
    domain::{PinRole, SiteKind},
};

pub(crate) fn should_route_device_net(net: &DeviceNet) -> bool {
    if net.origin_kind().is_synthetic_pad() {
        return false;
    }
    net.driver.as_ref().is_some_and(DeviceEndpoint::is_cell)
        && net.sinks.iter().any(DeviceEndpoint::is_cell)
}

pub(crate) fn endpoint_source_nets(cell: &DeviceCell, endpoint: &DeviceEndpoint) -> Vec<String> {
    match cell.site_kind_class() {
        SiteKind::LogicSlice => slice_source_nets(cell, endpoint),
        SiteKind::Iob => iob_source_nets(cell, endpoint),
        SiteKind::GclkIob => gclkiob_source_nets(cell, endpoint),
        SiteKind::Gclk => gclk_source_nets(cell, endpoint),
        SiteKind::Unknown => Vec::new(),
    }
}

pub(crate) fn endpoint_sink_nets(cell: &DeviceCell, endpoint: &DeviceEndpoint) -> Vec<String> {
    match cell.site_kind_class() {
        SiteKind::LogicSlice => slice_sink_nets(cell, endpoint),
        SiteKind::Iob => iob_sink_nets(cell, endpoint),
        SiteKind::Gclk => gclk_sink_nets(cell, endpoint),
        SiteKind::GclkIob | SiteKind::Unknown => Vec::new(),
    }
}

fn slice_source_nets(cell: &DeviceCell, endpoint: &DeviceEndpoint) -> Vec<String> {
    let slot = bel_slot(&cell.bel).unwrap_or(0);
    let pin_role = PinRole::classify_for_primitive(cell.primitive_kind(), &endpoint.pin);
    if pin_role == PinRole::RegisterOutput {
        return vec![if slot == 0 { "S0_XQ" } else { "S0_YQ" }.to_string()];
    }
    if pin_role == PinRole::LutOutput {
        return vec![if slot == 0 { "S0_X" } else { "S0_Y" }.to_string()];
    }
    Vec::new()
}

fn slice_sink_nets(cell: &DeviceCell, endpoint: &DeviceEndpoint) -> Vec<String> {
    let slot = bel_slot(&cell.bel).unwrap_or(0);
    let pin_role = PinRole::classify_for_primitive(cell.primitive_kind(), &endpoint.pin);
    if let Some(index) = pin_role.lut_input_index() {
        return vec![format!(
            "S0_{}_B{}",
            if slot == 0 { "F" } else { "G" },
            index + 1
        )];
    }
    if pin_role == PinRole::RegisterClock {
        return vec!["S0_CLK_B".to_string()];
    }
    if pin_role == PinRole::RegisterClockEnable {
        return vec!["S0_CE_B".to_string()];
    }
    if pin_role == PinRole::RegisterSetReset {
        return vec!["S0_SR_B".to_string()];
    }
    Vec::new()
}

fn iob_source_nets(cell: &DeviceCell, endpoint: &DeviceEndpoint) -> Vec<String> {
    let side = cell.tile_type.as_str();
    let index = iob_index(cell);
    if PinRole::classify_for_site(cell.site_kind_class(), &endpoint.pin).is_site_input() {
        return vec![format!("{side}_I{index}")];
    }
    Vec::new()
}

fn iob_sink_nets(cell: &DeviceCell, endpoint: &DeviceEndpoint) -> Vec<String> {
    let side = cell.tile_type.as_str();
    let index = iob_index(cell);
    if PinRole::classify_for_site(cell.site_kind_class(), &endpoint.pin).is_site_output() {
        return vec![format!("{side}_O{index}")];
    }
    Vec::new()
}

fn gclkiob_source_nets(cell: &DeviceCell, endpoint: &DeviceEndpoint) -> Vec<String> {
    if PinRole::classify_for_site(cell.site_kind_class(), &endpoint.pin).is_global_clock_output() {
        return vec![format!("{}_GCLK{}_PW", cell.tile_type, cell.z)];
    }
    Vec::new()
}

fn gclk_source_nets(cell: &DeviceCell, endpoint: &DeviceEndpoint) -> Vec<String> {
    if PinRole::classify_for_site(cell.site_kind_class(), &endpoint.pin).is_global_clock_output() {
        return vec![format!("{}_GCLK{}_PW", cell.tile_type, cell.z)];
    }
    Vec::new()
}

fn gclk_sink_nets(cell: &DeviceCell, endpoint: &DeviceEndpoint) -> Vec<String> {
    if PinRole::classify_for_site(cell.site_kind_class(), &endpoint.pin).is_global_clock_input() {
        return vec![format!("{}_GCLK{}", cell.tile_type, cell.z)];
    }
    Vec::new()
}

pub(crate) fn should_skip_unmapped_sink(cell: &DeviceCell, endpoint: &DeviceEndpoint) -> bool {
    cell.site_kind_class().is_logic_slice()
        && PinRole::classify_for_primitive(cell.primitive_kind(), &endpoint.pin)
            == PinRole::RegisterData
}

fn iob_index(cell: &DeviceCell) -> usize {
    cell.site_name
        .chars()
        .rev()
        .find(|ch| ch.is_ascii_digit())
        .and_then(|ch| ch.to_digit(10))
        .map(|digit| digit as usize)
        .unwrap_or(cell.z)
}

fn bel_slot(bel: &str) -> Option<usize> {
    bel.chars()
        .rev()
        .find(|ch| ch.is_ascii_digit())
        .and_then(|ch| ch.to_digit(10))
        .map(|digit| digit as usize)
}
