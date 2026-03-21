use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimingNode {
    pub id: String,
    pub arrival_ns: f64,
    pub required_ns: f64,
    pub slack_ns: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimingEdge {
    pub from: String,
    pub to: String,
    pub delay_ns: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimingGraph {
    #[serde(default)]
    pub nodes: Vec<TimingNode>,
    #[serde(default)]
    pub edges: Vec<TimingEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimingPath {
    pub category: String,
    pub endpoint: String,
    pub delay_ns: f64,
    #[serde(default)]
    pub hops: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimingSummary {
    #[serde(default)]
    pub critical_path_ns: f64,
    #[serde(default)]
    pub fmax_mhz: f64,
    #[serde(default)]
    pub top_paths: Vec<TimingPath>,
}
