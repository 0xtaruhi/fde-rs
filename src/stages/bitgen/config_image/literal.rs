use crate::cil::SiteFunction;

pub(crate) fn address_count(function: &SiteFunction) -> Option<usize> {
    function
        .srams
        .iter()
        .filter_map(|sram| sram.address.map(|address| address as usize))
        .max()
        .map(|max| max + 1)
}

pub(crate) fn evaluate_equation(raw: &str, width: usize) -> Option<Vec<u8>> {
    let value = raw.trim();
    if value == "0" {
        return Some(vec![0; width]);
    }
    if value == "1" {
        return Some(vec![1; width]);
    }
    parse_bit_literal(value, width)
}

pub(crate) fn parse_bit_literal(raw: &str, width: usize) -> Option<Vec<u8>> {
    let raw = raw.trim().replace('_', "");
    if raw.is_empty() {
        return None;
    }
    if let Some((_, value)) = raw.split_once('\'') {
        return parse_verilog_literal(value, width);
    }

    let parsed = if let Some(value) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        u128::from_str_radix(value, 16).ok()?
    } else if let Some(value) = raw.strip_prefix("0b").or_else(|| raw.strip_prefix("0B")) {
        u128::from_str_radix(value, 2).ok()?
    } else {
        raw.parse::<u128>().ok()?
    };
    Some(
        (0..width)
            .map(|index| ((parsed >> index) & 1) as u8)
            .collect(),
    )
}

pub(crate) fn parse_verilog_literal(raw: &str, width: usize) -> Option<Vec<u8>> {
    let mut chars = raw.chars();
    let radix = chars.next()?.to_ascii_lowercase();
    let digits = chars.as_str();
    let parsed = match radix {
        'h' => u128::from_str_radix(digits, 16).ok()?,
        'b' => u128::from_str_radix(digits, 2).ok()?,
        'd' => digits.parse::<u128>().ok()?,
        _ => return None,
    };
    Some(
        (0..width)
            .map(|index| ((parsed >> index) & 1) as u8)
            .collect(),
    )
}
