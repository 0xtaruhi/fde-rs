use crate::domain::ascii::trimmed_eq_ignore_ascii_case;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum PortDirection {
    #[default]
    Input,
    Output,
    Inout,
}

impl PortDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
            Self::Inout => "inout",
        }
    }

    pub fn is_input_like(&self) -> bool {
        matches!(self, Self::Input | Self::Inout)
    }

    pub fn is_output_like(&self) -> bool {
        matches!(self, Self::Output | Self::Inout)
    }
}

impl std::str::FromStr for PortDirection {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if trimmed_eq_ignore_ascii_case(value, "input") || trimmed_eq_ignore_ascii_case(value, "in")
        {
            Ok(Self::Input)
        } else if trimmed_eq_ignore_ascii_case(value, "output")
            || trimmed_eq_ignore_ascii_case(value, "out")
        {
            Ok(Self::Output)
        } else if trimmed_eq_ignore_ascii_case(value, "inout")
            || trimmed_eq_ignore_ascii_case(value, "io")
        {
            Ok(Self::Inout)
        } else {
            Err(anyhow::anyhow!("unknown port direction: {}", value.trim()))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Port {
    pub name: String,
    pub direction: PortDirection,
    #[serde(default)]
    pub width: usize,
    #[serde(default)]
    pub pin: Option<String>,
    #[serde(default)]
    pub x: Option<usize>,
    #[serde(default)]
    pub y: Option<usize>,
}

impl Port {
    pub fn new(name: impl Into<String>, direction: PortDirection) -> Self {
        Self {
            name: name.into(),
            direction,
            ..Self::default()
        }
    }

    pub fn input(name: impl Into<String>) -> Self {
        Self::new(name, PortDirection::Input)
    }

    pub fn output(name: impl Into<String>) -> Self {
        Self::new(name, PortDirection::Output)
    }

    pub fn inout(name: impl Into<String>) -> Self {
        Self::new(name, PortDirection::Inout)
    }

    pub fn with_pin(mut self, pin: impl Into<String>) -> Self {
        self.pin = Some(pin.into());
        self
    }

    pub fn at(mut self, x: usize, y: usize) -> Self {
        self.x = Some(x);
        self.y = Some(y);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::PortDirection;
    use std::str::FromStr;

    #[test]
    fn parses_port_direction_case_insensitively() {
        assert_eq!(
            PortDirection::from_str("input").ok(),
            Some(PortDirection::Input)
        );
        assert_eq!(
            PortDirection::from_str("OUT").ok(),
            Some(PortDirection::Output)
        );
        assert_eq!(
            PortDirection::from_str("Io").ok(),
            Some(PortDirection::Inout)
        );
        assert!(PortDirection::from_str("mystery").is_err());
    }
}
