#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlockRamRouteTarget {
    pub(crate) wire_name: String,
    pub(crate) row_offset: isize,
}

pub(crate) fn route_target(pin: &str) -> Option<BlockRamRouteTarget> {
    let raw_pin = normalized_route_pin(pin)?;
    Some(BlockRamRouteTarget {
        wire_name: format!("BRAM_{raw_pin}"),
        row_offset: row_offset_for_raw_pin(&raw_pin)?,
    })
}

fn normalized_route_pin(pin: &str) -> Option<String> {
    let pin = pin.trim();
    if let Some(raw) = pin.strip_prefix("BRAM_") {
        return Some(raw.to_ascii_uppercase());
    }

    if pin.eq_ignore_ascii_case("CLK") || pin.eq_ignore_ascii_case("CKA") {
        return Some("CLKA".to_string());
    }
    if pin.eq_ignore_ascii_case("WE") || pin.eq_ignore_ascii_case("AWE") {
        return Some("WEA".to_string());
    }
    if pin.eq_ignore_ascii_case("RST") {
        return Some("RSTA".to_string());
    }
    if pin.eq_ignore_ascii_case("EN")
        || pin.eq_ignore_ascii_case("ENA")
        || pin.eq_ignore_ascii_case("AEN")
    {
        return Some("SELA".to_string());
    }
    if pin.eq_ignore_ascii_case("CLKB") || pin.eq_ignore_ascii_case("CKB") {
        return Some("CLKB".to_string());
    }
    if pin.eq_ignore_ascii_case("WEB") || pin.eq_ignore_ascii_case("BWE") {
        return Some("WEB".to_string());
    }
    if pin.eq_ignore_ascii_case("RSTB") {
        return Some("RSTB".to_string());
    }
    if pin.eq_ignore_ascii_case("ENB") || pin.eq_ignore_ascii_case("BEN") {
        return Some("SELB".to_string());
    }
    if pin.eq_ignore_ascii_case("CLKA")
        || pin.eq_ignore_ascii_case("WEA")
        || pin.eq_ignore_ascii_case("RSTA")
        || pin.eq_ignore_ascii_case("SELA")
        || pin.eq_ignore_ascii_case("CLKB")
        || pin.eq_ignore_ascii_case("WEB")
        || pin.eq_ignore_ascii_case("RSTB")
        || pin.eq_ignore_ascii_case("SELB")
    {
        return Some(pin.to_ascii_uppercase());
    }
    if pin.eq_ignore_ascii_case("DI") {
        return Some("DIA0".to_string());
    }
    if pin.eq_ignore_ascii_case("DO") {
        return Some("DOA0".to_string());
    }
    if pin.eq_ignore_ascii_case("DIA") {
        return Some("DIA0".to_string());
    }
    if pin.eq_ignore_ascii_case("DOA") {
        return Some("DOA0".to_string());
    }
    if pin.eq_ignore_ascii_case("DIB") {
        return Some("DIB0".to_string());
    }
    if pin.eq_ignore_ascii_case("DOB") {
        return Some("DOB0".to_string());
    }

    if let Some(index) = parse_indexed_pin(pin, "DI") {
        return Some(format!("DIA{index}"));
    }
    if let Some(index) = parse_indexed_pin(pin, "DO") {
        return Some(format!("DOA{index}"));
    }
    if let Some(index) = parse_indexed_pin(pin, "ADDR") {
        return Some(format!("ADDRA{index}"));
    }
    if let Some(index) = parse_indexed_pin(pin, "DIA") {
        return Some(format!("DIA{index}"));
    }
    if let Some(index) = parse_indexed_pin(pin, "DOA") {
        return Some(format!("DOA{index}"));
    }
    if let Some(index) = parse_indexed_pin(pin, "ADDRA") {
        return Some(format!("ADDRA{index}"));
    }
    if let Some(index) = parse_indexed_pin(pin, "DIB") {
        return Some(format!("DIB{index}"));
    }
    if let Some(index) = parse_indexed_pin(pin, "DOB") {
        return Some(format!("DOB{index}"));
    }
    parse_indexed_pin(pin, "ADDRB").map(|index| format!("ADDRB{index}"))
}

fn row_offset_for_raw_pin(raw_pin: &str) -> Option<isize> {
    if matches!(raw_pin, "CLKA" | "WEA" | "SELA" | "RSTA") {
        return Some(-2);
    }
    if matches!(raw_pin, "CLKB" | "WEB" | "SELB" | "RSTB") {
        return Some(-1);
    }
    if let Some(index) =
        parse_indexed_pin(raw_pin, "ADDRA").or_else(|| parse_indexed_pin(raw_pin, "ADDRB"))
    {
        return Some((index / 4) as isize - 2);
    }
    if let Some(index) =
        parse_indexed_pin(raw_pin, "DOA").or_else(|| parse_indexed_pin(raw_pin, "DOB"))
    {
        return Some(index as isize % 4 - 3);
    }
    if let Some(index) =
        parse_indexed_pin(raw_pin, "DIA").or_else(|| parse_indexed_pin(raw_pin, "DIB"))
    {
        return Some(match index {
            0 | 2 | 8 | 10 => -3,
            1 | 3 | 9 | 11 => -2,
            4 | 5 | 12 | 13 => -1,
            6 | 7 | 14 | 15 => 0,
            _ => return None,
        });
    }
    None
}

fn parse_indexed_pin(pin: &str, prefix: &str) -> Option<usize> {
    let pin = pin.trim();
    if pin.len() < prefix.len() || !pin[..prefix.len()].eq_ignore_ascii_case(prefix) {
        return None;
    }
    let suffix = &pin[prefix.len()..];
    if suffix.starts_with('[') && suffix.ends_with(']') {
        return suffix[1..suffix.len() - 1].parse().ok();
    }
    (!suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()))
        .then(|| suffix.parse().ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::route_target;

    #[test]
    fn maps_single_port_block_ram_pins_to_cpp_style_route_targets() {
        let di0 = route_target("DI0").expect("DI0 target");
        let di = route_target("DI").expect("DI target");
        let do0 = route_target("DO").expect("DO target");
        let do14 = route_target("DO14").expect("DO14 target");
        let addr5 = route_target("ADDR5").expect("ADDR5 target");
        let en = route_target("EN").expect("EN target");

        assert_eq!(di.wire_name, "BRAM_DIA0");
        assert_eq!(di.row_offset, -3);
        assert_eq!(do0.wire_name, "BRAM_DOA0");
        assert_eq!(do0.row_offset, -3);
        assert_eq!(di0.wire_name, "BRAM_DIA0");
        assert_eq!(di0.row_offset, -3);
        assert_eq!(do14.wire_name, "BRAM_DOA14");
        assert_eq!(do14.row_offset, -1);
        assert_eq!(addr5.wire_name, "BRAM_ADDRA5");
        assert_eq!(addr5.row_offset, -1);
        assert_eq!(en.wire_name, "BRAM_SELA");
        assert_eq!(en.row_offset, -2);
    }

    #[test]
    fn maps_dual_port_block_ram_pins_to_segment_specific_route_targets() {
        let dia = route_target("DIA").expect("DIA target");
        let doa = route_target("DOA").expect("DOA target");
        let dib = route_target("DIB").expect("DIB target");
        let dob = route_target("DOB").expect("DOB target");
        let dia15 = route_target("DIA15").expect("DIA15 target");
        let dob5 = route_target("DOB5").expect("DOB5 target");
        let addrb0 = route_target("ADDRB0").expect("ADDRB0 target");
        let enb = route_target("ENB").expect("ENB target");

        assert_eq!(dia.wire_name, "BRAM_DIA0");
        assert_eq!(dia.row_offset, -3);
        assert_eq!(doa.wire_name, "BRAM_DOA0");
        assert_eq!(doa.row_offset, -3);
        assert_eq!(dib.wire_name, "BRAM_DIB0");
        assert_eq!(dib.row_offset, -3);
        assert_eq!(dob.wire_name, "BRAM_DOB0");
        assert_eq!(dob.row_offset, -3);
        assert_eq!(dia15.wire_name, "BRAM_DIA15");
        assert_eq!(dia15.row_offset, 0);
        assert_eq!(dob5.wire_name, "BRAM_DOB5");
        assert_eq!(dob5.row_offset, -2);
        assert_eq!(addrb0.wire_name, "BRAM_ADDRB0");
        assert_eq!(addrb0.row_offset, -2);
        assert_eq!(enb.wire_name, "BRAM_SELB");
        assert_eq!(enb.row_offset, -1);
    }
}
