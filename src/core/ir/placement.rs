use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cluster {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub capacity: usize,
    #[serde(default)]
    pub x: Option<usize>,
    #[serde(default)]
    pub y: Option<usize>,
    #[serde(default)]
    pub fixed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlacementSite {
    pub cluster: String,
    pub x: usize,
    pub y: usize,
    #[serde(default)]
    pub fixed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Placement {
    #[serde(default)]
    pub sites: Vec<PlacementSite>,
}
