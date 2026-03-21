use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Property {
    pub key: String,
    pub value: String,
}

impl Property {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CellPin {
    pub port: String,
    pub net: String,
}

impl CellPin {
    pub fn new(port: impl Into<String>, net: impl Into<String>) -> Self {
        Self {
            port: port.into(),
            net: net.into(),
        }
    }
}
