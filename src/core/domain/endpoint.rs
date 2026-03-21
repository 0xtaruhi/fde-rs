use super::ascii::trimmed_eq_ignore_ascii_case;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum EndpointKind {
    #[serde(rename = "cell")]
    Cell,
    #[serde(rename = "port")]
    Port,
    #[default]
    #[serde(rename = "unknown")]
    Unknown,
}

impl EndpointKind {
    pub fn classify(raw: &str) -> Self {
        if trimmed_eq_ignore_ascii_case(raw, "cell") {
            Self::Cell
        } else if trimmed_eq_ignore_ascii_case(raw, "port") {
            Self::Port
        } else {
            Self::Unknown
        }
    }

    pub fn is_cell(self) -> bool {
        matches!(self, Self::Cell)
    }

    pub fn is_port(self) -> bool {
        matches!(self, Self::Port)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cell => "cell",
            Self::Port => "port",
            Self::Unknown => "unknown",
        }
    }
}

impl FromStr for EndpointKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self::classify(value))
    }
}

impl From<&str> for EndpointKind {
    fn from(value: &str) -> Self {
        Self::classify(value)
    }
}

impl From<String> for EndpointKind {
    fn from(value: String) -> Self {
        Self::classify(&value)
    }
}

impl fmt::Display for EndpointKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::EndpointKind;

    #[test]
    fn classifies_endpoint_kinds_case_insensitively() {
        assert_eq!(EndpointKind::classify("cell"), EndpointKind::Cell);
        assert_eq!(EndpointKind::classify("PORT"), EndpointKind::Port);
        assert_eq!(EndpointKind::classify("mystery"), EndpointKind::Unknown);
    }
}
