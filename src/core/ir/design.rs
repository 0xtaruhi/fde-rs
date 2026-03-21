use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::{Cell, Cluster, DesignIndex, Net, Placement, PlacementSite, Port, TimingSummary};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Design {
    pub name: String,
    pub stage: String,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub ports: Vec<Port>,
    #[serde(default)]
    pub cells: Vec<Cell>,
    #[serde(default)]
    pub nets: Vec<Net>,
    #[serde(default)]
    pub clusters: Vec<Cluster>,
    #[serde(default)]
    pub timing: Option<TimingSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metadata {
    #[serde(default)]
    pub source_format: String,
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub arch_name: String,
    #[serde(default)]
    pub lut_size: usize,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl Design {
    pub fn index(&self) -> DesignIndex<'_> {
        DesignIndex::build(self)
    }

    pub fn note(&mut self, note: impl Into<String>) {
        self.metadata.notes.push(note.into());
    }

    pub fn used_pins(&self) -> BTreeSet<String> {
        self.ports
            .iter()
            .filter_map(|port| port.pin.clone())
            .collect::<BTreeSet<_>>()
    }

    pub fn placement(&self) -> Placement {
        Placement {
            sites: self
                .clusters
                .iter()
                .filter_map(|cluster| {
                    Some(PlacementSite {
                        cluster: cluster.name.clone(),
                        x: cluster.x?,
                        y: cluster.y?,
                        z: cluster.z.unwrap_or(0),
                        fixed: cluster.fixed,
                    })
                })
                .collect(),
        }
    }
}
