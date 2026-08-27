use crate::domain::TimingPathCategory;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TimingConstraintStatus {
    Met,
    Violated,
    PartiallyConstrained,
    #[default]
    Unconstrained,
    NotAnalyzed,
}

impl TimingConstraintStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Met => "MET",
            Self::Violated => "VIOLATED",
            Self::PartiallyConstrained => "PARTIALLY CONSTRAINED",
            Self::Unconstrained => "UNCONSTRAINED",
            Self::NotAnalyzed => "NOT ANALYZED",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TimingCheckKind {
    #[default]
    Setup,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TimingPointKind {
    Port,
    Net,
    CellArc,
    ClockToQ,
    SetupCheck,
    #[default]
    Endpoint,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TimingDelaySource {
    Constraint,
    CellLibrary,
    RoutedRc,
    DelayTable,
    GeometricEstimate,
    Constant,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimingPathPoint {
    pub kind: TimingPointKind,
    pub object: String,
    #[serde(default)]
    pub increment_ns: f64,
    #[serde(default)]
    pub cumulative_ns: f64,
    #[serde(default)]
    pub fanout: Option<usize>,
    #[serde(default)]
    pub delay_source: TimingDelaySource,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimingClockSummary {
    pub name: String,
    pub source: String,
    pub period_ns: f64,
    #[serde(default)]
    pub setup_uncertainty_ns: f64,
    #[serde(default)]
    pub register_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimingCheckSummary {
    pub status: TimingConstraintStatus,
    #[serde(default)]
    pub worst_slack_ns: Option<f64>,
    #[serde(default)]
    pub total_negative_slack_ns: f64,
    #[serde(default)]
    pub failing_endpoint_count: usize,
    #[serde(default)]
    pub analyzed_endpoint_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimingCoverage {
    #[serde(default)]
    pub register_endpoints: usize,
    #[serde(default)]
    pub constrained_register_endpoints: usize,
    #[serde(default)]
    pub primary_inputs: usize,
    #[serde(default)]
    pub constrained_primary_inputs: usize,
    #[serde(default)]
    pub primary_outputs: usize,
    #[serde(default)]
    pub constrained_primary_outputs: usize,
    #[serde(default)]
    pub modeled_arc_count: usize,
    #[serde(default)]
    pub fallback_arc_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimingPathGroupSummary {
    pub name: String,
    #[serde(default)]
    pub endpoint_count: usize,
    #[serde(default)]
    pub worst_slack_ns: Option<f64>,
    #[serde(default)]
    pub total_negative_slack_ns: f64,
    #[serde(default)]
    pub failing_endpoint_count: usize,
}

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
    pub category: TimingPathCategory,
    #[serde(default)]
    pub check: TimingCheckKind,
    #[serde(default)]
    pub startpoint: String,
    pub endpoint: String,
    #[serde(default)]
    pub path_group: String,
    #[serde(default)]
    pub launch_clock: Option<String>,
    #[serde(default)]
    pub capture_clock: Option<String>,
    pub delay_ns: f64,
    #[serde(default)]
    pub data_arrival_ns: f64,
    #[serde(default)]
    pub data_required_ns: Option<f64>,
    #[serde(default)]
    pub slack_ns: Option<f64>,
    #[serde(default)]
    pub logic_levels: usize,
    #[serde(default)]
    pub points: Vec<TimingPathPoint>,
    /// Compatibility-only flattened labels. New consumers should use `points`.
    #[serde(default)]
    pub hops: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimingSummary {
    #[serde(default)]
    pub constraint_status: TimingConstraintStatus,
    #[serde(default)]
    pub critical_path_ns: f64,
    #[serde(default)]
    pub fmax_mhz: f64,
    #[serde(default)]
    pub setup: TimingCheckSummary,
    #[serde(default)]
    pub hold: TimingCheckSummary,
    #[serde(default)]
    pub coverage: TimingCoverage,
    #[serde(default)]
    pub clocks: Vec<TimingClockSummary>,
    #[serde(default)]
    pub path_groups: Vec<TimingPathGroupSummary>,
    #[serde(default)]
    pub top_paths: Vec<TimingPath>,
}
