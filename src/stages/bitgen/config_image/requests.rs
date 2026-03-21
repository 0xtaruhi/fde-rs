use std::collections::BTreeMap;

use super::super::device::{DeviceCell, DeviceCellId, DeviceDesign, DeviceDesignIndex, DeviceEndpoint};
use super::{
    literal::{address_count, parse_bit_literal},
    lookup::{bel_slot, cell_property},
    resolve::resolve_site_config,
    types::{RequestedConfig, SiteInstance},
};
use crate::{
    cil::SiteDef,
    domain::{PrimitiveKind, SiteKind},
};

pub(crate) fn derive_site_requests(
    site: &SiteInstance,
    device: &DeviceDesign,
    index: &DeviceDesignIndex<'_>,
    site_def: &SiteDef,
) -> Vec<RequestedConfig> {
    match site.site_kind {
        SiteKind::LogicSlice => derive_slice_requests(site, site_def),
        SiteKind::Iob => derive_iob_requests(site, device, index),
        SiteKind::Gclk => vec![
            RequestedConfig {
                cfg_name: "CEMUX".to_string(),
                function_name: "1".to_string(),
            },
            RequestedConfig {
                cfg_name: "DISABLE_ATTR".to_string(),
                function_name: "LOW".to_string(),
            },
        ],
        SiteKind::GclkIob => vec![RequestedConfig {
            cfg_name: "IOATTRBOX".to_string(),
            function_name: "LVTTL".to_string(),
        }],
        SiteKind::Const | SiteKind::Unplaced | SiteKind::Unknown => Vec::new(),
    }
}

fn derive_slice_requests(site: &SiteInstance, site_def: &SiteDef) -> Vec<RequestedConfig> {
    let mut requests = Vec::new();
    let mut ff_slots = [false; 2];

    for cell in &site.cells {
        let slot = bel_slot(&cell.bel).unwrap_or(site.z.min(1)).min(1);
        let primitive = cell.primitive_kind();
        if primitive.is_lut()
            && let Some(function_name) = normalized_lut_function_name(cell, site_def, slot)
        {
            requests.push(RequestedConfig {
                cfg_name: if slot == 0 { "F" } else { "G" }.to_string(),
                function_name,
            });
            requests.push(RequestedConfig {
                cfg_name: if slot == 0 { "FXMUX" } else { "GYMUX" }.to_string(),
                function_name: if slot == 0 { "F" } else { "G" }.to_string(),
            });
        }
        if primitive.is_sequential() {
            ff_slots[slot] = true;
            requests.push(RequestedConfig {
                cfg_name: if slot == 0 { "FFX" } else { "FFY" }.to_string(),
                function_name: "#FF".to_string(),
            });
            requests.push(RequestedConfig {
                cfg_name: if slot == 0 { "INITX" } else { "INITY" }.to_string(),
                function_name: "LOW".to_string(),
            });
            requests.push(RequestedConfig {
                cfg_name: if slot == 0 { "SYNCX" } else { "SYNCY" }.to_string(),
                function_name: "ASYNC".to_string(),
            });
            requests.push(RequestedConfig {
                cfg_name: if slot == 0 { "DXMUX" } else { "DYMUX" }.to_string(),
                function_name: "1".to_string(),
            });
        }
    }

    if ff_slots.iter().any(|present| *present) {
        requests.push(RequestedConfig {
            cfg_name: "CKINV".to_string(),
            function_name: "1".to_string(),
        });
        requests.push(RequestedConfig {
            cfg_name: "SYNC_ATTR".to_string(),
            function_name: "ASYNC".to_string(),
        });
    } else if !site.cells.is_empty() {
        requests.push(RequestedConfig {
            cfg_name: "XUSED".to_string(),
            function_name: "0".to_string(),
        });
        requests.push(RequestedConfig {
            cfg_name: "YUSED".to_string(),
            function_name: "0".to_string(),
        });
    }

    dedup_requests(requests)
}

fn normalized_lut_function_name(
    cell: &DeviceCell,
    site_def: &SiteDef,
    slot: usize,
) -> Option<String> {
    let init = cell_property(cell, "lut_init")?;
    let cfg_name = if slot == 0 { "F" } else { "G" };
    let site_table_bits = site_truth_table_bits(site_def, cfg_name)?;
    let logical_table_bits = logical_truth_table_bits(cell.primitive_kind())?;
    let logical_bits = parse_bit_literal(init, logical_table_bits)?;

    let expanded_bits = if site_table_bits <= logical_bits.len() {
        logical_bits.into_iter().take(site_table_bits).collect::<Vec<_>>()
    } else {
        (0..site_table_bits)
            .map(|index| logical_bits[index % logical_bits.len()])
            .collect::<Vec<_>>()
    };

    Some(format_truth_table_literal(&expanded_bits))
}

fn site_truth_table_bits(site_def: &SiteDef, cfg_name: &str) -> Option<usize> {
    site_def
        .config_element(cfg_name)?
        .functions
        .iter()
        .filter_map(address_count)
        .max()
}

fn logical_truth_table_bits(primitive: PrimitiveKind) -> Option<usize> {
    let inputs = match primitive {
        PrimitiveKind::Lut { inputs: Some(inputs) } => inputs,
        PrimitiveKind::Lut { inputs: None } => return None,
        PrimitiveKind::FlipFlop
        | PrimitiveKind::Latch
        | PrimitiveKind::Constant(_)
        | PrimitiveKind::Buffer
        | PrimitiveKind::Io
        | PrimitiveKind::GlobalClockBuffer
        | PrimitiveKind::Generic
        | PrimitiveKind::Unknown => return None,
    };
    1usize.checked_shl(inputs as u32)
}

fn format_truth_table_literal(bits: &[u8]) -> String {
    let digit_count = bits.len().max(1).div_ceil(4);
    let mut digits = String::with_capacity(digit_count);
    for digit_index in (0..digit_count).rev() {
        let nibble = (0..4).fold(0u8, |value, bit_index| {
            let bit = bits
                .get(digit_index * 4 + bit_index)
                .copied()
                .unwrap_or(0)
                & 1;
            value | (bit << bit_index)
        });
        digits.push(match nibble {
            0..=9 => char::from(b'0' + nibble),
            10..=15 => char::from(b'A' + (nibble - 10)),
            _ => return "0x0".to_string(),
        });
    }
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        "0x0".to_string()
    } else {
        format!("0x{digits}")
    }
}

fn derive_iob_requests(
    site: &SiteInstance,
    device: &DeviceDesign,
    index: &DeviceDesignIndex<'_>,
) -> Vec<RequestedConfig> {
    let Some(cell) = site.cells.first() else {
        return Vec::new();
    };
    let Some(cell_id) = index.cell_id(&cell.cell_name) else {
        return Vec::new();
    };
    let mut requests = vec![RequestedConfig {
        cfg_name: "IOATTRBOX".to_string(),
        function_name: "LVTTL".to_string(),
    }];
    let input_used = device.nets.iter().any(|net| {
        net.driver
            .as_ref()
            .is_some_and(|driver| endpoint_matches_cell_pin(index, driver, cell_id, "IN"))
    });
    let output_used = device.nets.iter().any(|net| {
        net.sinks
            .iter()
            .any(|sink| endpoint_matches_cell_pin(index, sink, cell_id, "OUT"))
    });
    if input_used {
        requests.push(RequestedConfig {
            cfg_name: "IMUX".to_string(),
            function_name: "1".to_string(),
        });
    }
    if output_used {
        requests.push(RequestedConfig {
            cfg_name: "OMUX".to_string(),
            function_name: "O".to_string(),
        });
        requests.push(RequestedConfig {
            cfg_name: "OUTMUX".to_string(),
            function_name: "1".to_string(),
        });
        requests.push(RequestedConfig {
            cfg_name: "DRIVEATTRBOX".to_string(),
            function_name: "12".to_string(),
        });
        requests.push(RequestedConfig {
            cfg_name: "SLEW".to_string(),
            function_name: "SLOW".to_string(),
        });
    }
    dedup_requests(requests)
}

fn endpoint_matches_cell_pin(
    index: &DeviceDesignIndex<'_>,
    endpoint: &DeviceEndpoint,
    cell_id: DeviceCellId,
    pin_name: &str,
) -> bool {
    index
        .cell_for_endpoint(endpoint)
        .is_some_and(|endpoint_cell_id| endpoint_cell_id == cell_id)
        && endpoint.pin == pin_name
}

fn dedup_requests(requests: Vec<RequestedConfig>) -> Vec<RequestedConfig> {
    let mut deduped = BTreeMap::new();
    for (index, request) in requests.into_iter().enumerate() {
        deduped.insert(request.cfg_name.clone(), (index, request));
    }
    let mut ordered = deduped.into_values().collect::<Vec<_>>();
    ordered.sort_by_key(|(index, _)| *index);
    ordered.into_iter().map(|(_, request)| request).collect()
}

pub(crate) fn merge_site_requests(
    site_def: &SiteDef,
    explicit: Vec<RequestedConfig>,
) -> Vec<RequestedConfig> {
    dedup_requests(
        explicit
            .into_iter()
            .filter(|request| {
                matches!(
                    resolve_site_config(site_def, &request.cfg_name, &request.function_name),
                    super::ConfigResolution::Matched(_)
                )
            })
            .collect(),
    )
}
