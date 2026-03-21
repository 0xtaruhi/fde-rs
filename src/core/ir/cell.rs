use crate::domain::{CellKind, ConstantKind, PrimitiveKind};
use serde::{Deserialize, Serialize};

use super::{CellPin, Property};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cell {
    pub name: String,
    pub kind: CellKind,
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
    pub fn new(name: impl Into<String>, kind: CellKind, type_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind,
            type_name: type_name.into(),
            ..Self::default()
        }
    }

    pub fn lut(name: impl Into<String>, type_name: impl Into<String>) -> Self {
        Self::new(name, CellKind::Lut, type_name)
    }

    pub fn ff(name: impl Into<String>, type_name: impl Into<String>) -> Self {
        Self::new(name, CellKind::Ff, type_name)
    }

    pub fn with_input(mut self, port: impl Into<String>, net: impl Into<String>) -> Self {
        self.inputs.push(CellPin::new(port, net));
        self
    }

    pub fn with_output(mut self, port: impl Into<String>, net: impl Into<String>) -> Self {
        self.outputs.push(CellPin::new(port, net));
        self
    }

    pub fn in_cluster(mut self, cluster: impl Into<String>) -> Self {
        self.cluster = Some(cluster.into());
        self
    }

    pub fn primitive_kind(&self) -> PrimitiveKind {
        PrimitiveKind::from_cell_kind(self.kind, &self.type_name)
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
            self.properties.push(Property::new(key, value));
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
