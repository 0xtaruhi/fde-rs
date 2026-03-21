use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum NetOrigin {
    #[serde(rename = "logical-net")]
    Logical,
    #[serde(rename = "synthetic-pad-input")]
    SyntheticPadInput,
    #[serde(rename = "synthetic-pad-output")]
    SyntheticPadOutput,
    #[serde(rename = "synthetic-gclk")]
    SyntheticGclk,
    #[default]
    #[serde(rename = "unknown")]
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

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Logical => "logical-net",
            Self::SyntheticPadInput => "synthetic-pad-input",
            Self::SyntheticPadOutput => "synthetic-pad-output",
            Self::SyntheticGclk => "synthetic-gclk",
            Self::Unknown => "unknown",
        }
    }
}

impl FromStr for NetOrigin {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self::classify(value))
    }
}

impl From<&str> for NetOrigin {
    fn from(value: &str) -> Self {
        Self::classify(value)
    }
}

impl From<String> for NetOrigin {
    fn from(value: String) -> Self {
        Self::classify(&value)
    }
}

impl fmt::Display for NetOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
