#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetOrigin {
    Logical,
    SyntheticPadInput,
    SyntheticPadOutput,
    SyntheticGclk,
    Unknown,
}

impl NetOrigin {
    pub fn classify(raw: &str) -> Self {
        match raw.trim() {
            "logical-net" => Self::Logical,
            "synthetic-pad-input" => Self::SyntheticPadInput,
            "synthetic-pad-output" => Self::SyntheticPadOutput,
            "synthetic-gclk" => Self::SyntheticGclk,
            _ => Self::Unknown,
        }
    }

    pub fn is_synthetic_pad(self) -> bool {
        matches!(self, Self::SyntheticPadInput | Self::SyntheticPadOutput)
    }
}

#[cfg(test)]
mod tests {
    use super::NetOrigin;

    #[test]
    fn classifies_known_net_origins() {
        assert_eq!(NetOrigin::classify("logical-net"), NetOrigin::Logical);
        assert_eq!(
            NetOrigin::classify("synthetic-pad-input"),
            NetOrigin::SyntheticPadInput
        );
        assert_eq!(NetOrigin::classify("other"), NetOrigin::Unknown);
    }
}
