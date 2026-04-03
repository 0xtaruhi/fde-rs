use crate::ir::Cell;

pub(super) fn infer_lut_width(type_name: &str) -> usize {
    type_name
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or(2)
}

pub(super) fn canonicalize_lut_init(cell: &mut Cell, treat_bare_values_as_decimal: bool) {
    if let Some(value) = cell
        .property("lut_init")
        .or_else(|| cell.property("init"))
        .map(str::to_owned)
        && let Some(canonical) = canonical_lut_init_literal(
            &value,
            infer_lut_width(&cell.type_name).max(1),
            treat_bare_values_as_decimal,
        )
    {
        cell.set_property("lut_init", canonical);
    }
}

pub(super) fn default_lut_mask(width: usize) -> String {
    match width {
        0 | 1 => "0x2".to_string(),
        2 => "0x8".to_string(),
        3 => "0x80".to_string(),
        4 => "0x8000".to_string(),
        5 => "0x80000000".to_string(),
        _ => "0xAAAAAAAA".to_string(),
    }
}

pub(super) fn all_zeros_truth_table(lut_size: usize) -> String {
    format_lut_init_hex(0, lut_size)
}

pub(super) fn all_ones_truth_table(lut_size: usize) -> String {
    let bits = 1usize.checked_shl(lut_size.min(7) as u32).unwrap_or(128);
    if bits >= 128 {
        return format!("0x{:X}", u128::MAX);
    }
    format_lut_init_hex((1u128 << bits) - 1, lut_size)
}

pub(super) fn canonical_lut_init_literal(
    raw: &str,
    lut_width: usize,
    treat_bare_values_as_decimal: bool,
) -> Option<String> {
    let value = parse_lut_init_value(raw, treat_bare_values_as_decimal)?;
    Some(format_lut_init_hex(value, lut_width))
}

pub(super) fn parse_lut_init_value(raw: &str, treat_bare_values_as_decimal: bool) -> Option<u128> {
    let raw = raw.trim().replace('_', "");
    if raw.is_empty() {
        return None;
    }
    if let Some((_, value)) = raw.split_once('\'') {
        return parse_verilog_lut_init(value);
    }
    if let Some(value) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        return u128::from_str_radix(value, 16).ok();
    }
    if let Some(value) = raw.strip_prefix("0b").or_else(|| raw.strip_prefix("0B")) {
        return u128::from_str_radix(value, 2).ok();
    }
    if treat_bare_values_as_decimal {
        raw.parse::<u128>().ok()
    } else {
        u128::from_str_radix(&raw, 16).ok()
    }
}

fn parse_verilog_lut_init(raw: &str) -> Option<u128> {
    let mut chars = raw.chars();
    let radix = chars.next()?.to_ascii_lowercase();
    let digits = chars.as_str();
    match radix {
        'h' => u128::from_str_radix(digits, 16).ok(),
        'b' => u128::from_str_radix(digits, 2).ok(),
        'd' => digits.parse::<u128>().ok(),
        _ => None,
    }
}

pub(super) fn format_lut_init_hex(value: u128, lut_width: usize) -> String {
    let bit_count = 1usize.checked_shl(lut_width.min(7) as u32).unwrap_or(128);
    let masked = if bit_count >= 128 {
        value
    } else {
        value & ((1u128 << bit_count) - 1)
    };
    let digits = lut_hex_digits(lut_width);
    format!("0x{masked:0digits$X}")
}

fn lut_hex_digits(lut_width: usize) -> usize {
    match lut_width {
        0..=2 => 1,
        _ => 1usize << lut_width.saturating_sub(2).min(5),
    }
}
