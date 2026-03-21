use super::ascii::trimmed_eq_ignore_ascii_case;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum TimingPathCategory {
    #[serde(rename = "register-input")]
    RegisterInput,
    #[serde(rename = "combinational")]
    Combinational,
    #[serde(rename = "primary-output")]
    PrimaryOutput,
    #[serde(rename = "endpoint")]
    Endpoint,
    #[default]
    #[serde(rename = "unknown")]
    Unknown,
}

impl TimingPathCategory {
    pub fn classify(raw: &str) -> Self {
        if trimmed_eq_ignore_ascii_case(raw, "register-input") {
            Self::RegisterInput
        } else if trimmed_eq_ignore_ascii_case(raw, "combinational") {
            Self::Combinational
        } else if trimmed_eq_ignore_ascii_case(raw, "primary-output") {
            Self::PrimaryOutput
        } else if trimmed_eq_ignore_ascii_case(raw, "endpoint") {
            Self::Endpoint
        } else {
            Self::Unknown
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::RegisterInput => "register-input",
            Self::Combinational => "combinational",
            Self::PrimaryOutput => "primary-output",
            Self::Endpoint => "endpoint",
            Self::Unknown => "unknown",
        }
    }
}

impl FromStr for TimingPathCategory {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self::classify(value))
    }
}

impl From<&str> for TimingPathCategory {
    fn from(value: &str) -> Self {
        Self::classify(value)
    }
}

impl From<String> for TimingPathCategory {
    fn from(value: String) -> Self {
        Self::classify(&value)
    }
}

impl fmt::Display for TimingPathCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::TimingPathCategory;

    #[test]
    fn classifies_timing_path_categories_case_insensitively() {
        assert_eq!(
            TimingPathCategory::classify("REGISTER-INPUT"),
            TimingPathCategory::RegisterInput
        );
        assert_eq!(
            TimingPathCategory::classify("combinational"),
            TimingPathCategory::Combinational
        );
        assert_eq!(
            TimingPathCategory::classify("PRIMARY-OUTPUT"),
            TimingPathCategory::PrimaryOutput
        );
        assert_eq!(
            TimingPathCategory::classify("mystery"),
            TimingPathCategory::Unknown
        );
    }
}
