use crate::ir::TimingSummary;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path, time::Duration};

pub type ReportMetrics = BTreeMap<String, Value>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    #[default]
    Success,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StageReport {
    pub stage: String,
    #[serde(default)]
    pub status: ReportStatus,
    #[serde(default)]
    pub elapsed_ms: Option<u64>,
    #[serde(default)]
    pub messages: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub metrics: ReportMetrics,
    #[serde(default)]
    pub artifacts: BTreeMap<String, String>,
}

impl StageReport {
    pub fn new(stage: impl Into<String>) -> Self {
        Self {
            stage: stage.into(),
            status: ReportStatus::Success,
            elapsed_ms: None,
            messages: Vec::new(),
            warnings: Vec::new(),
            metrics: BTreeMap::new(),
            artifacts: BTreeMap::new(),
        }
    }

    pub fn push(&mut self, message: impl Into<String>) {
        self.messages.push(message.into());
    }

    pub fn warn(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    pub fn set_elapsed(&mut self, elapsed: Duration) {
        self.elapsed_ms = Some(elapsed.as_millis().try_into().unwrap_or(u64::MAX));
    }

    pub fn metric(&mut self, key: impl Into<String>, value: impl Serialize) {
        self.metrics.insert(
            key.into(),
            serde_json::to_value(value).expect("stage metric must serialize"),
        );
    }

    pub fn artifact(&mut self, key: impl Into<String>, path: impl AsRef<Path>) {
        self.artifacts
            .insert(key.into(), path.as_ref().display().to_string());
    }
}

#[derive(Debug, Clone)]
pub struct StageOutput<T> {
    pub value: T,
    pub report: StageReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImplementationReport {
    pub schema_version: u32,
    pub flow: String,
    pub design: String,
    pub out_dir: String,
    pub seed: u64,
    #[serde(default)]
    pub status: ReportStatus,
    #[serde(default)]
    pub elapsed_ms: Option<u64>,
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
    #[serde(default)]
    pub resources: BTreeMap<String, String>,
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
    let elapsed = report
        .elapsed_ms
        .map(format_elapsed_ms)
        .unwrap_or_else(|| "-".to_string());
    let metrics = format_metric_pairs(&report.metrics);
    if metrics.is_empty() {
        println!(
            "[{}] {} elapsed={}",
            report.stage,
            format_status(report.status),
            elapsed
        );
    } else {
        println!(
            "[{}] {} elapsed={} {}",
            report.stage,
            format_status(report.status),
            elapsed,
            metrics
        );
    }
    for warning in &report.warnings {
        println!("[{}][warn] {}", report.stage, warning);
    }
    for message in &report.messages {
        println!("[{}] {}", report.stage, message);
    }
}

pub fn render_summary_report(report: &ImplementationReport) -> String {
    let mut out = String::new();
    out.push_str("FDE Implementation Summary\n");
    out.push_str("==========================\n");
    out.push_str(&format!("Design         : {}\n", report.design));
    out.push_str(&format!(
        "Status         : {}\n",
        format_status(report.status)
    ));
    out.push_str(&format!("Seed           : {}\n", report.seed));
    if let Some(elapsed_ms) = report.elapsed_ms {
        out.push_str(&format!(
            "Total runtime  : {}\n",
            format_elapsed_ms(elapsed_ms)
        ));
    }
    if let Some(bitstream_sha256) = report.bitstream_sha256.as_deref() {
        out.push_str(&format!("Bitstream SHA  : {}\n", bitstream_sha256));
    }

    if !report.inputs.is_empty() {
        out.push_str("\nInputs\n------\n");
        for (key, value) in &report.inputs {
            out.push_str(&format!("{:14}: {}\n", key, value));
        }
    }

    if !report.resources.is_empty() {
        out.push_str("\nResources\n---------\n");
        for (key, value) in &report.resources {
            out.push_str(&format!("{:14}: {}\n", key, value));
        }
    }

    out.push_str("\nStage Runtime\n-------------\n");
    for stage in &report.stages {
        let elapsed = stage
            .elapsed_ms
            .map(format_elapsed_ms)
            .unwrap_or_else(|| "-".to_string());
        out.push_str(&format!("{:14}: {}\n", stage.stage, elapsed));
    }

    out.push_str("\nQoR Summary\n-----------\n");
    if let Some(stage) = report.stages.iter().find(|stage| stage.stage == "map") {
        push_metric_line(&mut out, "Mapped cells", stage.metrics.get("cell_count"));
        push_metric_line(&mut out, "Mapped nets", stage.metrics.get("net_count"));
    }
    if let Some(stage) = report.stages.iter().find(|stage| stage.stage == "pack") {
        push_metric_line(&mut out, "Clusters", stage.metrics.get("cluster_count"));
        push_metric_line(
            &mut out,
            "Cluster cap",
            stage.metrics.get("cluster_capacity"),
        );
    }
    if let Some(stage) = report.stages.iter().find(|stage| stage.stage == "place") {
        push_metric_line(&mut out, "Place cost", stage.metrics.get("final_cost"));
    }
    if let Some(stage) = report.stages.iter().find(|stage| stage.stage == "route") {
        push_metric_line(
            &mut out,
            "Route pips",
            stage.metrics.get("physical_pip_count"),
        );
        push_metric_line(
            &mut out,
            "Route sites",
            stage.metrics.get("routed_site_count"),
        );
        push_metric_line(
            &mut out,
            "Device nets",
            stage.metrics.get("device_net_count"),
        );
    }
    if let Some(timing) = report.timing.as_ref() {
        out.push_str(&format!(
            "{:14}: {:.3} ns\n",
            "Critical path", timing.critical_path_ns
        ));
        out.push_str(&format!("{:14}: {:.2} MHz\n", "Fmax", timing.fmax_mhz));
    }

    out
}

pub fn render_detailed_log(report: &ImplementationReport) -> String {
    let mut out = String::new();
    out.push_str("FDE Run Log\n");
    out.push_str("===========\n");
    out.push_str(&format!("Schema version : {}\n", report.schema_version));
    out.push_str(&format!("Flow           : {}\n", report.flow));
    out.push_str(&format!("Design         : {}\n", report.design));
    out.push_str(&format!(
        "Status         : {}\n",
        format_status(report.status)
    ));
    out.push_str(&format!("Seed           : {}\n", report.seed));
    if let Some(elapsed_ms) = report.elapsed_ms {
        out.push_str(&format!(
            "Total runtime  : {}\n",
            format_elapsed_ms(elapsed_ms)
        ));
    }
    out.push('\n');

    push_mapping_section(&mut out, "Inputs", &report.inputs);
    push_mapping_section(&mut out, "Resources", &report.resources);
    push_mapping_section(&mut out, "Artifacts", &report.artifacts);

    out.push_str("Stages\n------\n");
    for stage in &report.stages {
        let elapsed = stage
            .elapsed_ms
            .map(format_elapsed_ms)
            .unwrap_or_else(|| "-".to_string());
        out.push_str(&format!(
            "{} [{}] elapsed={}\n",
            stage.stage,
            format_status(stage.status),
            elapsed
        ));
        if !stage.metrics.is_empty() {
            out.push_str("  Metrics:\n");
            for (key, value) in &stage.metrics {
                out.push_str(&format!("    - {} = {}\n", key, format_metric_value(value)));
            }
        }
        if !stage.artifacts.is_empty() {
            out.push_str("  Artifacts:\n");
            for (key, value) in &stage.artifacts {
                out.push_str(&format!("    - {} = {}\n", key, value));
            }
        }
        if !stage.warnings.is_empty() {
            out.push_str("  Warnings:\n");
            for warning in &stage.warnings {
                out.push_str(&format!("    - {}\n", warning));
            }
        }
        if !stage.messages.is_empty() {
            out.push_str("  Messages:\n");
            for message in &stage.messages {
                out.push_str(&format!("    - {}\n", message));
            }
        }
        out.push('\n');
    }

    if let Some(timing) = report.timing.as_ref() {
        out.push_str("Timing\n------\n");
        out.push_str(&format!(
            "critical_path_ns = {:.6}\n",
            timing.critical_path_ns
        ));
        out.push_str(&format!("fmax_mhz         = {:.6}\n", timing.fmax_mhz));
        if !timing.top_paths.is_empty() {
            out.push_str("top_paths:\n");
            for (index, path) in timing.top_paths.iter().enumerate() {
                out.push_str(&format!(
                    "  {}. {:?} endpoint={} delay_ns={:.6}\n",
                    index + 1,
                    path.category,
                    path.endpoint,
                    path.delay_ns
                ));
            }
        }
    }

    out
}

fn push_mapping_section(out: &mut String, title: &str, values: &BTreeMap<String, String>) {
    if values.is_empty() {
        return;
    }
    out.push_str(title);
    out.push('\n');
    out.push_str(&"-".repeat(title.len()));
    out.push('\n');
    for (key, value) in values {
        out.push_str(&format!("{:14}: {}\n", key, value));
    }
    out.push('\n');
}

fn push_metric_line(out: &mut String, label: &str, value: Option<&Value>) {
    if let Some(value) = value {
        out.push_str(&format!("{:14}: {}\n", label, format_metric_value(value)));
    }
}

fn format_status(status: ReportStatus) -> &'static str {
    match status {
        ReportStatus::Success => "SUCCESS",
        ReportStatus::Failed => "FAILED",
        ReportStatus::Skipped => "SKIPPED",
    }
}

fn format_elapsed_ms(elapsed_ms: u64) -> String {
    if elapsed_ms == 0 {
        return "<1 ms".to_string();
    }
    if elapsed_ms >= 1_000 {
        format!("{:.3} s", elapsed_ms as f64 / 1_000.0)
    } else {
        format!("{elapsed_ms} ms")
    }
}

fn format_metric_pairs(metrics: &ReportMetrics) -> String {
    metrics
        .iter()
        .map(|(key, value)| format!("{key}={}", format_metric_value(value)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_metric_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => match (value.as_u64(), value.as_i64(), value.as_f64()) {
            (Some(value), _, _) => value.to_string(),
            (_, Some(value), _) => value.to_string(),
            (_, _, Some(value)) => {
                let rounded = format!("{value:.3}");
                rounded
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_string()
            }
            _ => value.to_string(),
        },
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}
