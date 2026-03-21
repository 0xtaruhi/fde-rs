use crate::domain::EndpointKind;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EndpointKey {
    Cell { name: String, pin: String },
    Port { name: String, pin: String },
    Unknown { name: String, pin: String },
}

impl EndpointKey {
    pub fn new(kind: EndpointKind, name: impl Into<String>, pin: impl Into<String>) -> Self {
        let name = name.into();
        let pin = pin.into();
        match kind {
            EndpointKind::Cell => Self::Cell { name, pin },
            EndpointKind::Port => Self::Port { name, pin },
            EndpointKind::Unknown => Self::Unknown { name, pin },
        }
    }

    pub fn endpoint_kind(&self) -> EndpointKind {
        match self {
            Self::Cell { .. } => EndpointKind::Cell,
            Self::Port { .. } => EndpointKind::Port,
            Self::Unknown { .. } => EndpointKind::Unknown,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Cell { name, .. } | Self::Port { name, .. } | Self::Unknown { name, .. } => name,
        }
    }

    pub fn pin(&self) -> &str {
        match self {
            Self::Cell { pin, .. } | Self::Port { pin, .. } | Self::Unknown { pin, .. } => pin,
        }
    }
}

impl From<&Endpoint> for EndpointKey {
    fn from(endpoint: &Endpoint) -> Self {
        Self::new(endpoint.kind, endpoint.name.clone(), endpoint.pin.clone())
    }
}

impl fmt::Display for EndpointKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}",
            self.endpoint_kind().as_str(),
            self.name(),
            self.pin()
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Endpoint {
    pub kind: EndpointKind,
    pub name: String,
    pub pin: String,
}

impl Endpoint {
    pub fn new(kind: EndpointKind, name: impl Into<String>, pin: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            pin: pin.into(),
        }
    }

    pub fn cell(name: impl Into<String>, pin: impl Into<String>) -> Self {
        Self::new(EndpointKind::Cell, name, pin)
    }

    pub fn port(name: impl Into<String>, pin: impl Into<String>) -> Self {
        Self::new(EndpointKind::Port, name, pin)
    }

    pub fn endpoint_kind(&self) -> EndpointKind {
        self.kind
    }

    pub fn is_cell(&self) -> bool {
        self.kind.is_cell()
    }

    pub fn is_port(&self) -> bool {
        self.kind.is_port()
    }

    pub fn key(&self) -> EndpointKey {
        EndpointKey::from(self)
    }
}

#[cfg(test)]
mod tests {
    use super::{Endpoint, EndpointKey};
    use crate::domain::EndpointKind;

    #[test]
    fn formats_typed_endpoint_keys_with_stable_shape() {
        let endpoint = Endpoint::new(EndpointKind::Cell, "u0", "O");
        let key = endpoint.key();
        assert_eq!(
            key,
            EndpointKey::Cell {
                name: "u0".to_string(),
                pin: "O".to_string()
            }
        );
        assert_eq!(key.to_string(), "cell:u0:O");
    }
}
