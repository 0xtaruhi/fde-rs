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
        match value.trim().to_ascii_lowercase().as_str() {
            "input" | "in" => Ok(Self::Input),
            "output" | "out" => Ok(Self::Output),
            "inout" | "io" => Ok(Self::Inout),
            other => Err(anyhow::anyhow!("unknown port direction: {other}")),
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
