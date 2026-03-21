use crate::domain::{ConstantKind, PrimitiveKind};
use serde::{Deserialize, Serialize};

use super::{CellPin, Property};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cell {
    pub name: String,
    pub kind: String,
    pub type_name: String,
    #[serde(default)]
    pub inputs: Vec<CellPin>,
    #[serde(default)]
    pub outputs: Vec<CellPin>,
    #[serde(default)]
    pub properties: Vec<Property>,
    #[serde(default)]
    pub cluster: Option<String>,
}

impl Cell {
    pub fn primitive_kind(&self) -> PrimitiveKind {
        PrimitiveKind::classify(&self.kind, &self.type_name)
    }

    pub fn constant_kind(&self) -> Option<ConstantKind> {
        self.primitive_kind().constant_kind()
    }

    pub fn property(&self, key: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|prop| prop.key.eq_ignore_ascii_case(key))
            .map(|prop| prop.value.as_str())
    }

    pub fn set_property(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        if let Some(existing) = self
            .properties
            .iter_mut()
            .find(|prop| prop.key.eq_ignore_ascii_case(&key))
        {
            existing.key = key;
            existing.value = value;
        } else {
            self.properties.push(Property { key, value });
        }
    }

    pub fn is_sequential(&self) -> bool {
        self.primitive_kind().is_sequential()
    }

    pub fn is_lut(&self) -> bool {
        self.primitive_kind().is_lut()
    }

    pub fn is_constant_source(&self) -> bool {
        self.primitive_kind().is_constant_source()
    }

    pub fn is_buffer(&self) -> bool {
        self.primitive_kind().is_buffer()
    }
}
