#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SiteKind {
    LogicSlice,
    Iob,
    GclkIob,
    Gclk,
    Unknown,
}

impl SiteKind {
    pub fn classify(raw: &str) -> Self {
        match raw.trim().to_ascii_uppercase().as_str() {
            "SLICE" => Self::LogicSlice,
            "IOB" => Self::Iob,
            "GCLKIOB" => Self::GclkIob,
            "GCLK" => Self::Gclk,
            _ => Self::Unknown,
        }
    }

    pub fn is_logic_slice(self) -> bool {
        matches!(self, Self::LogicSlice)
    }
}

#[cfg(test)]
mod tests {
    use super::SiteKind;

    #[test]
    fn classifies_known_site_kinds() {
        assert_eq!(SiteKind::classify("slice"), SiteKind::LogicSlice);
        assert_eq!(SiteKind::classify("IOB"), SiteKind::Iob);
        assert_eq!(SiteKind::classify("gclkiob"), SiteKind::GclkIob);
        assert_eq!(SiteKind::classify("nope"), SiteKind::Unknown);
    }
}
