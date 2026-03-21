use crate::{
    cil::{SiteDef, SiteFunction},
    config_image::{
        literal::{address_count, evaluate_equation, parse_bit_literal},
        types::{ConfigResolution, ResolvedSiteBit},
    },
};

pub(crate) fn resolve_site_config(
    site_def: &SiteDef,
    cfg_name: &str,
    function_name: &str,
) -> ConfigResolution {
    let Some(cfg) = site_def.config_element(cfg_name) else {
        return ConfigResolution::Unmatched;
    };
    let exact = cfg.function(function_name);
    let prefer_equation = prefers_equation_request(function_name);
    let Some(function) = exact.or_else(|| {
        if prefer_equation {
            cfg.computation_function("equation")
                .or_else(|| cfg.computation_function("srambit"))
        } else {
            cfg.computation_function("srambit")
                .or_else(|| cfg.computation_function("equation"))
        }
    }) else {
        return ConfigResolution::Unmatched;
    };

    match expand_function(function, cfg_name, function_name) {
        Some(bits) => ConfigResolution::Matched(bits),
        None => ConfigResolution::Unmatched,
    }
}

fn prefers_equation_request(requested_name: &str) -> bool {
    let value = requested_name.trim();
    value.starts_with("#LUT:")
        || value.starts_with("D=")
        || value.contains('A')
        || value.contains('*')
        || value.contains('+')
        || value.contains('~')
        || value.contains('(')
        || value.contains(')')
}

pub(crate) fn default_site_bits(site_def: &SiteDef) -> Vec<ResolvedSiteBit> {
    let mut bits = Vec::new();
    for cfg in &site_def.config_elements {
        let Some(function) = cfg.default_function() else {
            continue;
        };
        if let Some(expanded) = expand_function(function, &cfg.name, &function.name) {
            bits.extend(expanded);
        }
    }
    bits
}

fn expand_function(
    function: &SiteFunction,
    cfg_name: &str,
    requested_name: &str,
) -> Option<Vec<ResolvedSiteBit>> {
    let quomodo = if function.quomodo.is_empty() {
        "naming"
    } else {
        function.quomodo.as_str()
    };
    match quomodo {
        "naming" => Some(
            function
                .srams
                .iter()
                .map(|sram| ResolvedSiteBit {
                    cfg_name: cfg_name.to_string(),
                    function_name: requested_name.to_string(),
                    basic_cell: sram.basic_cell.clone(),
                    sram_name: sram.name.clone(),
                    value: sram.content,
                })
                .collect(),
        ),
        "srambit" => {
            let bits = parse_bit_literal(requested_name, address_count(function)?)?;
            Some(expand_addressed_bits(
                function,
                cfg_name,
                requested_name,
                &bits,
            ))
        }
        "equation" => {
            let bits = evaluate_equation(requested_name, address_count(function)?)?;
            Some(expand_addressed_bits(
                function,
                cfg_name,
                requested_name,
                &bits,
            ))
        }
        _ => None,
    }
}

fn expand_addressed_bits(
    function: &SiteFunction,
    cfg_name: &str,
    requested_name: &str,
    bits: &[u8],
) -> Vec<ResolvedSiteBit> {
    function
        .srams
        .iter()
        .map(|sram| ResolvedSiteBit {
            cfg_name: cfg_name.to_string(),
            function_name: requested_name.to_string(),
            basic_cell: sram.basic_cell.clone(),
            sram_name: sram.name.clone(),
            value: sram
                .address
                .and_then(|address| bits.get(address as usize).copied())
                .unwrap_or(sram.content),
        })
        .collect()
}
