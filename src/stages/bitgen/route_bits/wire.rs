pub(crate) fn parse_wire_index(raw: &str) -> Option<usize> {
    raw.parse().ok()
}

pub(crate) fn parse_indexed_wire(raw: &str) -> Option<(String, usize)> {
    for prefix in [
        "LEFT_LLV",
        "RIGHT_LLV",
        "TOP_LLV",
        "BOT_LLV",
        "LEFT_LLH",
        "RIGHT_LLH",
        "LEFT_E",
        "RIGHT_W",
        "TOP_S",
        "BOT_N",
        "LLV",
        "LLH",
        "E",
        "W",
        "N",
        "S",
        "LEFT_H6E",
        "LEFT_H6M",
        "RIGHT_H6W",
        "RIGHT_H6M",
        "TOP_V6S",
        "BOT_V6N",
        "H6E",
        "H6M",
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
            "LEFT_LLV" => "LEFT_LLV",
            "RIGHT_LLV" => "RIGHT_LLV",
            "TOP_LLV" => "TOP_LLV",
            "BOT_LLV" => "BOT_LLV",
            "LEFT_LLH" => "LEFT_LLH",
            "RIGHT_LLH" => "RIGHT_LLH",
            "LEFT_E" => "E",
            "RIGHT_W" => "W",
            "TOP_S" => "S",
            "BOT_N" => "N",
            "LEFT_H6E" => "H6E",
            "LEFT_H6M" => "H6M",
            "RIGHT_H6W" => "H6W",
            "RIGHT_H6M" => "H6M",
            "TOP_V6S" => "V6S",
            "BOT_V6N" => "V6N",
            other => other,
        };
        return Some((canonical.to_string(), index));
    }
    None
}

pub(crate) fn tile_distance(x0: usize, y0: usize, x1: usize, y1: usize) -> usize {
    x0.abs_diff(x1) + y0.abs_diff(y1)
}
