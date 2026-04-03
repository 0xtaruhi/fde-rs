use super::super::literal::{address_count, evaluate_equation, parse_bit_literal};
use crate::cil::{SiteDef, SiteFunction};

use super::types::{ConfigResolution, ResolvedSiteBit};

pub(crate) fn resolve_site_config(
    site_def: &SiteDef,
    cfg_name: &str,
    function_name: &str,
) -> ConfigResolution {
    let Some(cfg) = site_def.config_element(cfg_name) else {
        return ConfigResolution::Unmatched;
    };
    for function in [
        cfg.function(function_name),
        cfg.computation_function("srambit"),
        cfg.computation_function("equation"),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(bits) = expand_function(function, cfg_name, function_name) {
            return ConfigResolution::Matched(bits);
        }
    }
    ConfigResolution::Unmatched
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
