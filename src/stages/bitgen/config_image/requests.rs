use crate::{
    cil::SiteDef,
    config_image::{
        literal::{address_count, lut_hex_to_equation, parse_bit_literal},
        lookup::{bel_slot, cell_property, is_ff_type, is_lut_type},
        types::{RequestedConfig, SiteInstance},
    },
    device::DeviceDesign,
};
use std::collections::BTreeMap;

pub(crate) fn derive_site_requests(
    site: &SiteInstance,
    device: &DeviceDesign,
    site_def: &SiteDef,
) -> Vec<RequestedConfig> {
    match site.site_kind.as_str() {
        "SLICE" => derive_slice_requests(site, site_def),
        "IOB" => derive_iob_requests(site, device),
        "GCLK" => vec![
            RequestedConfig {
                cfg_name: "CEMUX".to_string(),
                function_name: "1".to_string(),
            },
            RequestedConfig {
                cfg_name: "DISABLE_ATTR".to_string(),
                function_name: "LOW".to_string(),
            },
        ],
        "GCLKIOB" => vec![RequestedConfig {
            cfg_name: "IOATTRBOX".to_string(),
            function_name: "LVTTL".to_string(),
        }],
        _ => Vec::new(),
    }
}

fn derive_slice_requests(site: &SiteInstance, site_def: &SiteDef) -> Vec<RequestedConfig> {
    let mut requests = Vec::new();
    let mut has_ff = false;

    for cell in &site.cells {
        let slot = bel_slot(&cell.bel).unwrap_or(site.z.min(1));
        let lut_cfg_name = if slot == 0 { "F" } else { "G" };
        if is_lut_type(&cell.type_name)
            && let Some(init) = canonical_lut_function(site_def, lut_cfg_name, cell)
        {
            requests.push(RequestedConfig {
                cfg_name: lut_cfg_name.to_string(),
                function_name: init,
            });
            requests.push(RequestedConfig {
                cfg_name: if slot == 0 { "FXMUX" } else { "GYMUX" }.to_string(),
                function_name: if slot == 0 { "F" } else { "G" }.to_string(),
            });
        }
        if is_ff_type(&cell.type_name) {
            has_ff = true;
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

    if has_ff {
        requests.push(RequestedConfig {
            cfg_name: "CKINV".to_string(),
            function_name: "1".to_string(),
        });
    }

    dedup_requests(requests)
}

fn derive_iob_requests(site: &SiteInstance, device: &DeviceDesign) -> Vec<RequestedConfig> {
    let Some(cell) = site.cells.first() else {
        return Vec::new();
    };
    let mut requests = vec![RequestedConfig {
        cfg_name: "IOATTRBOX".to_string(),
        function_name: "LVTTL".to_string(),
    }];
    let input_used = device.nets.iter().any(|net| {
        net.driver.as_ref().is_some_and(|driver| {
            driver.kind == "cell" && driver.name == cell.cell_name && driver.pin == "IN"
        })
    });
    let output_used = device.nets.iter().any(|net| {
        net.sinks
            .iter()
            .any(|sink| sink.kind == "cell" && sink.name == cell.cell_name && sink.pin == "OUT")
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

fn dedup_requests(requests: Vec<RequestedConfig>) -> Vec<RequestedConfig> {
    let mut deduped = BTreeMap::new();
    for request in requests {
        deduped.insert(request.cfg_name.clone(), request);
    }
    deduped.into_values().collect()
}

fn canonical_lut_function(
    site_def: &SiteDef,
    cfg_name: &str,
    cell: &crate::device::DeviceCell,
) -> Option<String> {
    let raw = cell_property(cell, "lut_init")?;
    let cfg = site_def.config_element(cfg_name);
    if cfg
        .and_then(|cfg| cfg.computation_function("equation"))
        .is_some()
    {
        if raw.trim_start().starts_with("#LUT:") {
            return Some(raw.to_string());
        }
        let source_width = infer_lut_width(&cell.type_name);
        let expr = lut_hex_to_equation(raw, source_width)?;
        return Some(format!("#LUT:D={expr}"));
    }

    let source_width = infer_lut_width(&cell.type_name);
    let source_bits = 1usize.checked_shl(source_width.min(7) as u32)?;
    let target_bits = cfg
        .and_then(|cfg| {
            cfg.computation_function("srambit")
                .or_else(|| cfg.computation_function("equation"))
        })
        .and_then(address_count)
        .unwrap_or(source_bits);
    if target_bits <= source_bits {
        return Some(raw.to_string());
    }
    let bits = parse_bit_literal(raw, source_bits)?;
    let expanded = (0..target_bits)
        .map(|index| bits[index % source_bits])
        .collect::<Vec<_>>();
    Some(bits_to_hex_literal(&expanded))
}

fn infer_lut_width(type_name: &str) -> usize {
    type_name
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or(2)
        .max(1)
}

fn bits_to_hex_literal(bits: &[u8]) -> String {
    let value = bits.iter().enumerate().fold(0u128, |acc, (index, bit)| {
        acc | (u128::from(*bit != 0) << index)
    });
    format!("0x{value:X}")
}
