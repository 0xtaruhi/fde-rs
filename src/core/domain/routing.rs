#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SliceOutputWireKind {
    LutX,
    LutY,
    RegisterX,
    RegisterY,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SliceControlWireKind {
    Clock,
    ClockEnable,
    SetReset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SliceHalf {
    X,
    Y,
}

pub fn is_dedicated_clock_wire_name(raw: &str) -> bool {
    raw.contains("GCLK")
}

pub fn is_clock_distribution_wire_name(raw: &str) -> bool {
    is_dedicated_clock_wire_name(raw) || raw.contains("CLKV") || raw.contains("CLKC")
}

pub fn is_clock_sink_wire_name(raw: &str) -> bool {
    raw.ends_with("_CLK_B")
}

pub fn is_pad_stub_wire_name(raw: &str) -> bool {
    raw.contains("_P")
}

pub fn is_hex_like_wire_name(raw: &str) -> bool {
    raw.contains("H6") || raw.contains("V6")
}

pub fn is_long_wire_name(raw: &str) -> bool {
    raw.contains("LLH")
        || raw.contains("LLV")
        || raw.starts_with("LH")
        || raw.starts_with("LV")
        || raw.starts_with("LEFT_LLH")
        || raw.starts_with("RIGHT_LLH")
        || raw.starts_with("TOP_LLV")
        || raw.starts_with("BOT_LLV")
}

pub fn is_directional_channel_wire_name(raw: &str) -> bool {
    matches!(raw.chars().next(), Some('N' | 'S' | 'E' | 'W'))
}

pub fn slice_output_wire_kind(raw: &str) -> Option<SliceOutputWireKind> {
    match raw {
        value if value.ends_with("_XQ") => Some(SliceOutputWireKind::RegisterX),
        value if value.ends_with("_YQ") => Some(SliceOutputWireKind::RegisterY),
        value if value.ends_with("_X") => Some(SliceOutputWireKind::LutX),
        value if value.ends_with("_Y") => Some(SliceOutputWireKind::LutY),
        _ => None,
    }
}

pub fn output_wire_index(raw: &str) -> Option<usize> {
    raw.strip_prefix("OUT")?.parse::<usize>().ok()
}

pub fn sink_output_preference(raw: &str) -> Option<usize> {
    if raw.ends_with("_O1") {
        Some(1)
    } else if raw.ends_with("_O2") {
        Some(2)
    } else {
        None
    }
}

pub fn normalized_slice_site_name(site_name: &str) -> &str {
    if site_name
        .strip_prefix('S')
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit()))
    {
        site_name
    } else {
        "S0"
    }
}

pub fn slice_register_output_wire_name(site_name: &str, slot: usize) -> String {
    let prefix = normalized_slice_site_name(site_name);
    match slice_half(slot) {
        SliceHalf::X => format!("{prefix}_XQ"),
        SliceHalf::Y => format!("{prefix}_YQ"),
    }
}

pub fn slice_lut_output_wire_name(site_name: &str, slot: usize) -> String {
    let prefix = normalized_slice_site_name(site_name);
    match slice_half(slot) {
        SliceHalf::X => format!("{prefix}_X"),
        SliceHalf::Y => format!("{prefix}_Y"),
    }
}

pub fn slice_lut_input_wire_prefix(site_name: &str, slot: usize) -> String {
    let prefix = normalized_slice_site_name(site_name);
    match slice_half(slot) {
        SliceHalf::X => format!("{prefix}_F_B"),
        SliceHalf::Y => format!("{prefix}_G_B"),
    }
}

pub fn slice_control_wire_name(site_name: &str, kind: SliceControlWireKind) -> String {
    let prefix = normalized_slice_site_name(site_name);
    let suffix = match kind {
        SliceControlWireKind::Clock => "CLK_B",
        SliceControlWireKind::ClockEnable => "CE_B",
        SliceControlWireKind::SetReset => "SR_B",
    };
    format!("{prefix}_{suffix}")
}

pub fn slice_register_data_wire_name(site_name: &str, slot: usize) -> String {
    let prefix = normalized_slice_site_name(site_name);
    match slice_half(slot) {
        SliceHalf::X => format!("{prefix}_BX_B"),
        SliceHalf::Y => format!("{prefix}_BY_B"),
    }
}

pub fn pin_map_property_name(logical_index: usize) -> String {
    format!("pin_map_ADR{logical_index}")
}

fn slice_half(slot: usize) -> SliceHalf {
    if slot == 0 {
        SliceHalf::X
    } else {
        SliceHalf::Y
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SliceControlWireKind, SliceOutputWireKind, is_clock_distribution_wire_name,
        is_clock_sink_wire_name, is_dedicated_clock_wire_name, is_directional_channel_wire_name,
        is_hex_like_wire_name, is_long_wire_name, normalized_slice_site_name, output_wire_index,
        pin_map_property_name, sink_output_preference, slice_control_wire_name,
        slice_lut_input_wire_prefix, slice_lut_output_wire_name, slice_output_wire_kind,
        slice_register_data_wire_name, slice_register_output_wire_name,
    };

    #[test]
    fn classifies_route_wire_name_semantics() {
        assert!(is_dedicated_clock_wire_name("CLKB_GCLK0"));
        assert!(is_clock_distribution_wire_name("CLKV_VGCLK0"));
        assert!(is_clock_sink_wire_name("S0_CLK_B"));
        assert!(is_hex_like_wire_name("H6W6"));
        assert!(is_long_wire_name("LEFT_LLH3"));
        assert!(is_directional_channel_wire_name("N8"));
        assert_eq!(
            slice_output_wire_kind("S0_XQ"),
            Some(SliceOutputWireKind::RegisterX)
        );
        assert_eq!(output_wire_index("OUT4"), Some(4));
        assert_eq!(sink_output_preference("LEFT_O2"), Some(2));
        assert_eq!(normalized_slice_site_name("S12"), "S12");
        assert_eq!(normalized_slice_site_name("SLICE0"), "S0");
        assert_eq!(slice_register_output_wire_name("S0", 0), "S0_XQ");
        assert_eq!(slice_lut_output_wire_name("S0", 1), "S0_Y");
        assert_eq!(slice_lut_input_wire_prefix("SLICE0", 1), "S0_G_B");
        assert_eq!(
            slice_control_wire_name("S1", SliceControlWireKind::ClockEnable),
            "S1_CE_B"
        );
        assert_eq!(slice_register_data_wire_name("S1", 1), "S1_BY_B");
        assert_eq!(pin_map_property_name(2), "pin_map_ADR2");
    }
}
