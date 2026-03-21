use crate::domain::EndpointKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Endpoint {
    pub kind: String,
    pub name: String,
    pub pin: String,
}

impl Endpoint {
    pub fn endpoint_kind(&self) -> EndpointKind {
        EndpointKind::classify(&self.kind)
    }

    pub fn is_cell(&self) -> bool {
        self.endpoint_kind().is_cell()
    }

    pub fn is_port(&self) -> bool {
        self.endpoint_kind().is_port()
    }

    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.kind, self.name, self.pin)
    }
}
