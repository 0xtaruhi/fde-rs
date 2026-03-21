pub(crate) fn trimmed_eq_ignore_ascii_case(raw: &str, expected: &str) -> bool {
    raw.trim().eq_ignore_ascii_case(expected)
}

pub(crate) fn trimmed_starts_with_ignore_ascii_case(raw: &str, prefix: &str) -> bool {
    let raw = raw.trim();
    raw.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

pub(crate) fn trimmed_contains_ignore_ascii_case(raw: &str, needle: &str) -> bool {
    let raw = raw.trim();
    if needle.is_empty() {
        return true;
    }
    if raw.len() < needle.len() {
        return false;
    }

    raw.as_bytes()
        .windows(needle.len())
        .any(|window| ascii_bytes_eq_ignore_case(window, needle.as_bytes()))
}

pub(crate) fn trimmed_strip_prefix_ignore_ascii_case<'a>(
    raw: &'a str,
    prefix: &str,
) -> Option<&'a str> {
    let raw = raw.trim();
    raw.get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| &raw[prefix.len()..])
}

fn ascii_bytes_eq_ignore_case(lhs: &[u8], rhs: &[u8]) -> bool {
    lhs.len() == rhs.len()
        && lhs
            .iter()
            .zip(rhs.iter())
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
}
