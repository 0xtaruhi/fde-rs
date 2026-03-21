use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::{Cell, Cluster, Net, Placement, PlacementSite, Port, TimingSummary};

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
    pub fn port_lookup(&self, name: &str) -> Option<&Port> {
        self.ports.iter().find(|port| port.name == name)
    }

    pub fn cell_lookup(&self, name: &str) -> Option<&Cell> {
        self.cells.iter().find(|cell| cell.name == name)
    }

    pub fn net_lookup(&self, name: &str) -> Option<&Net> {
        self.nets.iter().find(|net| net.name == name)
    }

    pub fn cell_map(&self) -> BTreeMap<&str, &Cell> {
        self.cells
            .iter()
            .map(|cell| (cell.name.as_str(), cell))
            .collect()
    }

    pub fn cluster_map(&self) -> BTreeMap<&str, &Cluster> {
        self.clusters
            .iter()
            .map(|cluster| (cluster.name.as_str(), cluster))
            .collect()
    }

    pub fn cluster_lookup(&self, cell_name: &str) -> Option<&Cluster> {
        let cluster_name = self
            .cells
            .iter()
            .find(|cell| cell.name == cell_name)
            .and_then(|cell| cell.cluster.as_deref())?;
        self.clusters
            .iter()
            .find(|cluster| cluster.name == cluster_name)
    }

    pub fn cell_output_nets<'a>(&'a self, cell_name: &str) -> Vec<&'a str> {
        self.cells
            .iter()
            .find(|cell| cell.name == cell_name)
            .map(|cell| cell.outputs.iter().map(|pin| pin.net.as_str()).collect())
            .unwrap_or_default()
    }

    pub fn driver_to_nets(&self) -> BTreeMap<String, Vec<String>> {
        let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for net in &self.nets {
            if let Some(driver) = &net.driver {
                map.entry(driver.key()).or_default().push(net.name.clone());
            }
        }
        map
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
                        fixed: cluster.fixed,
                    })
                })
                .collect(),
        }
    }
}
