use crate::{
    domain::{BlockRamPin, PinRole, SliceSlot},
    ir::{Design, DesignIndex, Endpoint},
};
use std::collections::BTreeMap;

use super::super::writer::{
    PhysicalEndpoint, PhysicalNet, PortInstanceBinding, SliceCellBinding, pin_map_indices,
};
use super::ports::split_clock_route_pips;

type PortLookup<'a> = BTreeMap<&'a str, &'a PortInstanceBinding>;

#[derive(Clone, Copy)]
struct PhysicalNetContext<'a> {
    design: &'a Design,
    index: &'a DesignIndex<'a>,
    cell_bindings: &'a BTreeMap<String, (String, SliceCellBinding)>,
    block_ram_bindings: &'a BTreeMap<String, String>,
    port_lookup: &'a PortLookup<'a>,
}

pub(super) fn build_physical_nets(
    design: &Design,
    index: &DesignIndex<'_>,
    cell_bindings: &BTreeMap<String, (String, SliceCellBinding)>,
    block_ram_bindings: &BTreeMap<String, String>,
    port_bindings: &[PortInstanceBinding],
) -> Vec<PhysicalNet> {
    let port_lookup = port_lookup(port_bindings);
    let context = PhysicalNetContext {
        design,
        index,
        cell_bindings,
        block_ram_bindings,
        port_lookup: &port_lookup,
    };
    let mut nets = build_internal_physical_nets(context);
    nets.extend(build_port_physical_nets(design, port_bindings));
    nets
}

fn port_lookup(port_bindings: &[PortInstanceBinding]) -> PortLookup<'_> {
    port_bindings
        .iter()
        .map(|binding| (binding.port_name.as_str(), binding))
        .collect()
}

fn build_internal_physical_nets(context: PhysicalNetContext<'_>) -> Vec<PhysicalNet> {
    context
        .design
        .nets
        .iter()
        .filter_map(|net| build_internal_physical_net(net, context))
        .collect()
}

fn build_internal_physical_net(
    net: &crate::ir::Net,
    context: PhysicalNetContext<'_>,
) -> Option<PhysicalNet> {
    let driver_port_binding = net
        .driver
        .as_ref()
        .and_then(|driver| resolved_port_binding(driver, context));
    let sink_port_binding = net
        .sinks
        .iter()
        .find_map(|sink| resolved_port_binding(sink, context));
    let driver_slice_binding = net
        .driver
        .as_ref()
        .and_then(|driver| resolved_slice_binding(driver, context));
    let endpoints = internal_net_endpoints(net, context, driver_slice_binding);
    if endpoints.len() < 2 {
        return None;
    }

    Some(PhysicalNet {
        name: physical_internal_net_name(net, driver_port_binding, sink_port_binding),
        net_type: is_clock_buffer_binding(driver_port_binding).then_some("clock"),
        endpoints,
        pips: internal_net_pips(net, driver_port_binding),
    })
}

fn resolved_port_binding<'a>(
    endpoint: &Endpoint,
    context: PhysicalNetContext<'a>,
) -> Option<&'a PortInstanceBinding> {
    match context.index.resolve_endpoint(endpoint) {
        crate::ir::EndpointTarget::Port(port_id) => {
            let port = context.index.port(context.design, port_id);
            context.port_lookup.get(port.name.as_str()).copied()
        }
        crate::ir::EndpointTarget::Cell(_) | crate::ir::EndpointTarget::Unknown => None,
    }
}

fn resolved_slice_binding(
    endpoint: &Endpoint,
    context: PhysicalNetContext<'_>,
) -> Option<SliceCellBinding> {
    match context.index.resolve_endpoint(endpoint) {
        crate::ir::EndpointTarget::Cell(cell_id) => {
            let cell = context.index.cell(context.design, cell_id);
            context
                .cell_bindings
                .get(cell.name.as_str())
                .map(|(_, binding)| *binding)
        }
        crate::ir::EndpointTarget::Port(_) | crate::ir::EndpointTarget::Unknown => None,
    }
}

fn internal_net_endpoints(
    net: &crate::ir::Net,
    context: PhysicalNetContext<'_>,
    driver_slice_binding: Option<SliceCellBinding>,
) -> Vec<PhysicalEndpoint> {
    let mut endpoints = Vec::new();
    if let Some(driver) = &net.driver
        && let Some(endpoint) = physical_driver_endpoint(driver, context)
    {
        push_unique_endpoint(&mut endpoints, endpoint);
    }
    for sink in &net.sinks {
        for endpoint in
            physical_sink_endpoints(sink, net.driver.as_ref(), context, driver_slice_binding)
        {
            push_unique_endpoint(&mut endpoints, endpoint);
        }
    }
    endpoints
}

fn is_clock_buffer_binding(binding: Option<&PortInstanceBinding>) -> bool {
    binding.is_some_and(|binding| binding.clock_input && binding.gclk_instance_name.is_some())
}

fn internal_net_pips(
    net: &crate::ir::Net,
    driver_port_binding: Option<&PortInstanceBinding>,
) -> Vec<crate::ir::RoutePip> {
    driver_port_binding
        .filter(|binding| is_clock_buffer_binding(Some(binding)))
        .map(|binding| split_clock_route_pips(&net.route_pips, binding).0)
        .unwrap_or_else(|| net.route_pips.clone())
}

fn build_port_physical_nets(
    design: &Design,
    port_bindings: &[PortInstanceBinding],
) -> Vec<PhysicalNet> {
    port_bindings
        .iter()
        .flat_map(|binding| port_physical_nets_for_binding(design, binding))
        .collect()
}

fn port_physical_nets_for_binding(
    design: &Design,
    binding: &PortInstanceBinding,
) -> Vec<PhysicalNet> {
    let mut nets = Vec::new();
    if binding.input_used {
        nets.push(input_port_pad_net(binding));
        if let Some(gclk_net) = input_clock_helper_net(design, binding) {
            nets.push(gclk_net);
        }
    }
    if binding.output_used {
        nets.push(output_port_pad_net(binding));
    }
    nets
}

fn input_port_pad_net(binding: &PortInstanceBinding) -> PhysicalNet {
    PhysicalNet {
        name: binding.port_name.clone(),
        net_type: None,
        endpoints: vec![
            PhysicalEndpoint {
                pin: binding.port_name.clone(),
                instance_ref: None,
            },
            PhysicalEndpoint {
                pin: "PAD".to_string(),
                instance_ref: Some(binding.pad_instance_name.clone()),
            },
        ],
        pips: Vec::new(),
    }
}

fn input_clock_helper_net(design: &Design, binding: &PortInstanceBinding) -> Option<PhysicalNet> {
    let gclk_instance_name = binding.gclk_instance_name.as_ref()?;
    Some(PhysicalNet {
        name: format!("net_Buf-pad-{}", binding.port_name),
        net_type: is_routed_physical_stage(design).then_some("clock"),
        endpoints: vec![
            PhysicalEndpoint {
                pin: "GCLKOUT".to_string(),
                instance_ref: Some(binding.pad_instance_name.clone()),
            },
            PhysicalEndpoint {
                pin: "IN".to_string(),
                instance_ref: Some(gclk_instance_name.clone()),
            },
        ],
        pips: helper_clock_pips(design, binding),
    })
}

fn helper_clock_pips(design: &Design, binding: &PortInstanceBinding) -> Vec<crate::ir::RoutePip> {
    if !is_routed_physical_stage(design) {
        return Vec::new();
    }
    design
        .nets
        .iter()
        .find(|net| net.name == binding.port_name)
        .map(|net| split_clock_route_pips(&net.route_pips, binding).1)
        .unwrap_or_default()
}

fn output_port_pad_net(binding: &PortInstanceBinding) -> PhysicalNet {
    PhysicalNet {
        name: binding.port_name.clone(),
        net_type: None,
        endpoints: vec![
            PhysicalEndpoint {
                pin: "PAD".to_string(),
                instance_ref: Some(binding.pad_instance_name.clone()),
            },
            PhysicalEndpoint {
                pin: binding.port_name.clone(),
                instance_ref: None,
            },
        ],
        pips: Vec::new(),
    }
}

fn is_routed_physical_stage(design: &Design) -> bool {
    matches!(design.stage.as_str(), "routed" | "timed")
}

fn physical_internal_net_name(
    net: &crate::ir::Net,
    driver_port_binding: Option<&PortInstanceBinding>,
    sink_port_binding: Option<&PortInstanceBinding>,
) -> String {
    if let Some(binding) = driver_port_binding {
        if binding.clock_input && binding.gclk_instance_name.is_some() {
            return format!("net_IBuf-clkpad-{}", binding.port_name);
        }
        return format!("net_Buf-pad-{}", binding.port_name);
    }
    if let Some(binding) = sink_port_binding {
        return format!("net_Buf-pad-{}", binding.port_name);
    }
    net.name.clone()
}

fn physical_driver_endpoint(
    endpoint: &Endpoint,
    context: PhysicalNetContext<'_>,
) -> Option<PhysicalEndpoint> {
    match endpoint.kind {
        crate::domain::EndpointKind::Cell => {
            let cell = context
                .index
                .cell_id(&endpoint.name)
                .map(|cell_id| context.index.cell(context.design, cell_id))?;
            if let Some((instance_name, binding)) = context.cell_bindings.get(cell.name.as_str()) {
                let slot = SliceSlot::from_index(binding.slot.min(1))?;
                let pin = match PinRole::classify_output_pin(cell.primitive_kind(), &endpoint.pin) {
                    PinRole::RegisterOutput => slot.register_output_pin().to_string(),
                    PinRole::LutOutput => slot.lut_output_pin().to_string(),
                    _ => return None,
                };
                return Some(PhysicalEndpoint {
                    pin,
                    instance_ref: Some(instance_name.clone()),
                });
            }
            let instance_name = context.block_ram_bindings.get(cell.name.as_str())?;
            let pin = physical_block_ram_pin_name(&endpoint.pin)?;
            Some(PhysicalEndpoint {
                pin,
                instance_ref: Some(instance_name.clone()),
            })
        }
        crate::domain::EndpointKind::Port => {
            let binding = context.port_lookup.get(endpoint.name.as_str())?;
            if let Some(gclk_instance_name) = binding.gclk_instance_name.as_ref() {
                Some(PhysicalEndpoint {
                    pin: "OUT".to_string(),
                    instance_ref: Some(gclk_instance_name.clone()),
                })
            } else {
                Some(PhysicalEndpoint {
                    pin: "IN".to_string(),
                    instance_ref: Some(binding.pad_instance_name.clone()),
                })
            }
        }
        crate::domain::EndpointKind::Unknown => None,
    }
}

fn physical_sink_endpoints(
    endpoint: &Endpoint,
    driver: Option<&Endpoint>,
    context: PhysicalNetContext<'_>,
    driver_slice_binding: Option<SliceCellBinding>,
) -> Vec<PhysicalEndpoint> {
    match endpoint.kind {
        crate::domain::EndpointKind::Cell => {
            let Some(cell) = context
                .index
                .cell_id(&endpoint.name)
                .map(|cell_id| context.index.cell(context.design, cell_id))
            else {
                return Vec::new();
            };
            if let Some((instance_name, binding)) = context.cell_bindings.get(cell.name.as_str()) {
                let Some(slot) = SliceSlot::from_index(binding.slot.min(1)) else {
                    return Vec::new();
                };
                return match PinRole::classify_for_primitive(cell.primitive_kind(), &endpoint.pin) {
                    PinRole::LutInput(logical_index) => pin_map_indices(cell, logical_index)
                        .into_iter()
                        .map(|physical_index| PhysicalEndpoint {
                            pin: slot.lut_input_pin(physical_index),
                            instance_ref: Some(instance_name.clone()),
                        })
                        .collect(),
                    PinRole::RegisterClock => vec![PhysicalEndpoint {
                        pin: "CLK".to_string(),
                        instance_ref: Some(instance_name.clone()),
                    }],
                    PinRole::RegisterClockEnable => vec![PhysicalEndpoint {
                        pin: "CE".to_string(),
                        instance_ref: Some(instance_name.clone()),
                    }],
                    PinRole::RegisterSetReset => vec![PhysicalEndpoint {
                        pin: "SR".to_string(),
                        instance_ref: Some(instance_name.clone()),
                    }],
                    PinRole::RegisterData => {
                        if register_uses_local_lut(driver, context, *binding, driver_slice_binding)
                        {
                            Vec::new()
                        } else {
                            vec![PhysicalEndpoint {
                                pin: slot.bypass_function_name().to_string(),
                                instance_ref: Some(instance_name.clone()),
                            }]
                        }
                    }
                    _ => Vec::new(),
                };
            }
            let Some(instance_name) = context.block_ram_bindings.get(cell.name.as_str()) else {
                return Vec::new();
            };
            physical_block_ram_pin_name(&endpoint.pin)
                .map(|pin| PhysicalEndpoint {
                    pin,
                    instance_ref: Some(instance_name.clone()),
                })
                .into_iter()
                .collect()
        }
        crate::domain::EndpointKind::Port => context
            .port_lookup
            .get(endpoint.name.as_str())
            .map(|binding| PhysicalEndpoint {
                pin: "OUT".to_string(),
                instance_ref: Some(binding.pad_instance_name.clone()),
            })
            .into_iter()
            .collect(),
        crate::domain::EndpointKind::Unknown => Vec::new(),
    }
}

fn register_uses_local_lut(
    driver: Option<&Endpoint>,
    context: PhysicalNetContext<'_>,
    sink_binding: SliceCellBinding,
    driver_binding: Option<SliceCellBinding>,
) -> bool {
    let Some(driver) = driver else {
        return false;
    };
    let crate::domain::EndpointKind::Cell = driver.kind else {
        return false;
    };
    let Some(driver_cell) = context
        .index
        .cell_id(&driver.name)
        .map(|cell_id| context.index.cell(context.design, cell_id))
    else {
        return false;
    };
    let Some((_, binding)) = context.cell_bindings.get(driver_cell.name.as_str()) else {
        return driver_binding.is_some_and(|binding| {
            driver_cell.is_lut() && binding.slot.min(1) == sink_binding.slot.min(1)
        });
    };
    driver_cell.is_lut() && binding.slot.min(1) == sink_binding.slot.min(1)
}

fn physical_block_ram_pin_name(pin: &str) -> Option<String> {
    BlockRamPin::parse(pin)?.physical_port_name()
}

fn push_unique_endpoint(endpoints: &mut Vec<PhysicalEndpoint>, endpoint: PhysicalEndpoint) {
    if !endpoints.contains(&endpoint) {
        endpoints.push(endpoint);
    }
}
