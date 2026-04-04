use super::{PhysicalInstance, SliceState};
use crate::ir::{Cell, Endpoint, Net, RoutePip, RouteSegment};
use roxmltree::Node;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn physical_stage_note(stage: &str) -> &'static str {
    match stage {
        "packed" => "Imported FDE packed XML",
        "placed" => "Imported FDE placed XML",
        _ => "Imported FDE routed XML",
    }
}

pub(super) fn is_pad_connection_net(name: &str, port_names: &BTreeSet<String>) -> bool {
    port_names.contains(name)
}

pub(super) fn is_clock_bridge_net(
    name: &str,
    clock_buffer_ports: &BTreeMap<String, String>,
) -> bool {
    name.strip_prefix("net_Buf-pad-").is_some_and(|port_name| {
        clock_buffer_ports
            .values()
            .any(|candidate| candidate == port_name)
    })
}

pub(super) fn logical_net_name<'a>(
    physical_name: &'a str,
    port_names: &BTreeSet<String>,
) -> &'a str {
    physical_name
        .strip_prefix("net_IBuf-clkpad-")
        .filter(|name| port_names.contains(*name))
        .or_else(|| {
            physical_name
                .strip_prefix("net_Buf-pad-")
                .filter(|name| port_names.contains(*name))
        })
        .unwrap_or(physical_name)
}

pub(super) fn inject_local_lut_ff_nets(
    slice_states: &BTreeMap<String, SliceState>,
    nets: &mut Vec<Net>,
) {
    for state in slice_states.values() {
        for slot in 0..2 {
            let Some(lut_name) = state.slots[slot].lut_name.as_ref() else {
                continue;
            };
            let Some(ff_name) = state.slots[slot].ff_name.as_ref() else {
                continue;
            };
            if !state.slots[slot].ff_uses_local_lut {
                continue;
            }
            let sink = Endpoint::cell(ff_name.clone(), "D");
            if let Some(existing) = nets.iter_mut().find(|net| {
                net.driver.as_ref().is_some_and(|driver| {
                    driver.kind == crate::domain::EndpointKind::Cell
                        && driver.name == *lut_name
                        && driver.pin.eq_ignore_ascii_case("O")
                })
            }) {
                push_unique_endpoint(&mut existing.sinks, sink);
                continue;
            }
            nets.push(
                Net::new(format!("{}::lut{slot}_to_ff{slot}", state.instance_name))
                    .with_driver(Endpoint::cell(lut_name.clone(), "O"))
                    .with_sink(sink),
            );
        }
    }
}

pub(super) fn attach_cell_pins(cells: &mut [Cell], nets: &[Net]) {
    let cells_by_name = cells
        .iter()
        .enumerate()
        .map(|(index, cell)| (cell.name.clone(), index))
        .collect::<BTreeMap<_, _>>();
    for net in nets {
        if let Some(driver) = &net.driver
            && driver.kind == crate::domain::EndpointKind::Cell
            && let Some(&cell_index) = cells_by_name.get(&driver.name)
        {
            let cell = &mut cells[cell_index];
            if !cell
                .outputs
                .iter()
                .any(|pin| pin.port == driver.pin && pin.net == net.name)
            {
                cell.outputs.push(crate::ir::CellPin::new(
                    driver.pin.clone(),
                    net.name.clone(),
                ));
            }
        }
        for sink in &net.sinks {
            if sink.kind != crate::domain::EndpointKind::Cell {
                continue;
            }
            let Some(&cell_index) = cells_by_name.get(&sink.name) else {
                continue;
            };
            let cell = &mut cells[cell_index];
            if !cell
                .inputs
                .iter()
                .any(|pin| pin.port == sink.pin && pin.net == net.name)
            {
                cell.inputs
                    .push(crate::ir::CellPin::new(sink.pin.clone(), net.name.clone()));
            }
        }
    }
}

pub(super) fn infer_physical_stage(instances: &[PhysicalInstance], nets: &[Net]) -> String {
    if nets.iter().any(|net| !net.route_pips.is_empty()) {
        return "routed".to_string();
    }
    if instances.iter().any(|instance| instance.position.is_some()) {
        return "placed".to_string();
    }
    "packed".to_string()
}

pub(super) fn route_pip(pip: Node<'_, '_>) -> Option<RoutePip> {
    let (x, y) = pip_position(pip)?;
    Some(RoutePip::new(
        (x, y),
        pip.attribute("from")?.to_string(),
        pip.attribute("to")?.to_string(),
    ))
}

pub(super) fn merge_route_pips(
    helper_pips: &[RoutePip],
    route_pips: Vec<RoutePip>,
) -> Vec<RoutePip> {
    let mut merged = helper_pips.to_vec();
    for pip in route_pips {
        if !merged.contains(&pip) {
            merged.push(pip);
        }
    }
    merged
}

pub(super) fn derive_segments_from_pips(pips: &[RoutePip]) -> Vec<RouteSegment> {
    let mut positions = Vec::<(usize, usize)>::new();
    for pip in pips {
        let position = (pip.x, pip.y);
        if positions.last().copied() != Some(position) {
            positions.push(position);
        }
    }
    match positions.as_slice() {
        [] => Vec::new(),
        [single] => vec![RouteSegment::new(*single, *single)],
        _ => positions
            .windows(2)
            .filter_map(|window| match window {
                [start, end] => Some(RouteSegment::new(*start, *end)),
                _ => None,
            })
            .collect(),
    }
}

pub(super) fn slice_instance_sort_key(name: &str) -> (usize, &str) {
    let index = name
        .strip_prefix("iSlice__")
        .and_then(|value| value.strip_suffix("__"))
        .and_then(|value| value.parse().ok())
        .unwrap_or(usize::MAX);
    (index, name)
}

pub(super) fn instance_position(instance: Node<'_, '_>) -> Option<(usize, usize, usize)> {
    instance
        .children()
        .find(|node| node.has_tag_name("property") && node.attribute("name") == Some("position"))
        .and_then(|property| property.attribute("value"))
        .and_then(super::parse_point)
}

pub(super) fn pip_position(pip: Node<'_, '_>) -> Option<(usize, usize)> {
    let value = pip.attribute("position")?;
    let mut parts = value.split(',').map(str::trim);
    let x = parts.next()?.parse().ok()?;
    let y = parts.next()?.parse().ok()?;
    Some((x, y))
}

pub(super) fn push_unique_endpoint(endpoints: &mut Vec<Endpoint>, endpoint: Endpoint) {
    if endpoints
        .iter()
        .any(|existing| existing.key() == endpoint.key())
    {
        return;
    }
    endpoints.push(endpoint);
}
