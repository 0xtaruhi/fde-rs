pub(crate) fn parse_wire_index(raw: &str) -> Option<usize> {
    raw.parse().ok()
}

#[cfg(test)]
pub(crate) fn parse_indexed_wire(raw: &str) -> Option<(&'static str, usize)> {
    for prefix in [
        "LEFT_E",
        "RIGHT_W",
        "TOP_S",
        "BOT_N",
        "E",
        "W",
        "N",
        "S",
        "LEFT_H6E",
        "RIGHT_H6W",
        "TOP_V6S",
        "BOT_V6N",
        "H6E",
        "H6W",
        "V6N",
        "V6S",
    ] {
        let Some(value) = raw.strip_prefix(prefix) else {
            continue;
        };
        let Ok(index) = value.parse::<usize>() else {
            continue;
        };
        let canonical = match prefix {
            "LEFT_E" => "E",
            "RIGHT_W" => "W",
            "TOP_S" => "S",
            "BOT_N" => "N",
            "LEFT_H6E" => "H6E",
            "RIGHT_H6W" => "H6W",
            "TOP_V6S" => "V6S",
            "BOT_V6N" => "V6N",
            other => other,
        };
        return Some((canonical, index));
    }
    None
}

pub(crate) fn tile_distance(x0: usize, y0: usize, x1: usize, y1: usize) -> usize {
    x0.abs_diff(x1) + y0.abs_diff(y1)
}

pub(crate) fn step_cost(raw: &str, programmable: bool) -> usize {
    let base = if raw.contains("LLH")
        || raw.contains("LLV")
        || raw.starts_with("LH")
        || raw.starts_with("LV")
    {
        1
    } else if raw.contains("H6") || raw.contains("V6") {
        2
    } else if is_single_channel_wire(raw) {
        4
    } else {
        3
    };
    base + usize::from(programmable)
}

fn is_single_channel_wire(raw: &str) -> bool {
    raw.starts_with("E")
        || raw.starts_with("W")
        || raw.starts_with("N")
        || raw.starts_with("S")
        || raw.starts_with("LEFT_E")
        || raw.starts_with("RIGHT_W")
        || raw.starts_with("TOP_S")
        || raw.starts_with("BOT_N")
}
