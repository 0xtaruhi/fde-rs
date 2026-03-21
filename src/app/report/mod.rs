use crate::ir::TimingSummary;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StageReport {
    pub stage: String,
    #[serde(default)]
    pub messages: Vec<String>,
}

impl StageReport {
    pub fn new(stage: impl Into<String>) -> Self {
        Self {
            stage: stage.into(),
            messages: Vec::new(),
        }
    }

    pub fn push(&mut self, message: impl Into<String>) {
        self.messages.push(message.into());
    }
}

#[derive(Debug, Clone)]
pub struct StageOutput<T> {
    pub value: T,
    pub report: StageReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImplementationReport {
    pub design: String,
    pub out_dir: String,
    pub seed: u64,
    #[serde(default)]
    pub artifacts: BTreeMap<String, String>,
    #[serde(default)]
    pub stages: Vec<StageReport>,
    #[serde(default)]
    pub timing: Option<TimingSummary>,
    #[serde(default)]
    pub bitstream_sha256: Option<String>,
}

pub fn print_stage_report(report: &StageReport) {
    for message in &report.messages {
        println!("[{}] {}", report.stage, message);
    }
}
