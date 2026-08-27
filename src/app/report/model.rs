use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Note,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Diagnostic {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<String>,
}

impl Diagnostic {
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: DiagnosticSeverity::Warning,
            message: message.into(),
            detail: None,
            help: None,
            object: None,
            artifact: None,
        }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: DiagnosticSeverity::Error,
            message: message.into(),
            detail: None,
            help: None,
            object: None,
            artifact: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_object(mut self, object: impl Into<String>) -> Self {
        self.object = Some(object.into());
        self
    }

    pub fn with_artifact(mut self, artifact: impl Into<String>) -> Self {
        self.artifact = Some(artifact.into());
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkUnit {
    Iterations,
    Nets,
    Passes,
}

impl WorkUnit {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Iterations => "iterations",
            Self::Nets => "nets",
            Self::Passes => "passes",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProgressUpdate {
    pub phase: String,
    pub current: u64,
    pub total: u64,
    pub unit: WorkUnit,
    #[serde(default)]
    pub metrics: BTreeMap<String, String>,
}

impl ProgressUpdate {
    pub fn new(phase: impl Into<String>, current: usize, total: usize, unit: WorkUnit) -> Self {
        Self {
            phase: phase.into(),
            current: current.try_into().unwrap_or(u64::MAX),
            total: total.try_into().unwrap_or(u64::MAX),
            unit,
            metrics: BTreeMap::new(),
        }
    }

    pub fn metric(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metrics.insert(key.into(), value.into());
        self
    }

    pub fn percent(&self) -> f64 {
        if self.total == 0 {
            100.0
        } else {
            self.current as f64 / self.total as f64 * 100.0
        }
    }
}
