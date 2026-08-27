mod modern;
mod options;
mod report;
mod resources;

use crate::report::ImplementationReport;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub use options::ImplementationOptions;

#[derive(Debug, thiserror::Error)]
#[error("{source:#}")]
pub struct ImplementationRunError {
    #[source]
    source: anyhow::Error,
    partial_report: Option<PathBuf>,
    exit_code: i32,
}

impl ImplementationRunError {
    pub(crate) fn with_partial_report(source: anyhow::Error, path: PathBuf) -> Self {
        Self {
            source,
            partial_report: Some(path),
            exit_code: 1,
        }
    }

    pub(crate) fn timing_violation(source: anyhow::Error, path: PathBuf) -> Self {
        Self {
            source,
            partial_report: Some(path),
            exit_code: 5,
        }
    }

    pub fn partial_report(&self) -> Option<&Path> {
        self.partial_report.as_deref()
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }
}

pub fn run(options: &ImplementationOptions) -> Result<ImplementationReport> {
    modern::run(options)
}

pub fn run_with_reporter(
    options: &ImplementationOptions,
    reporter: &mut dyn crate::report::StageReporter,
) -> Result<ImplementationReport> {
    modern::run_with_reporter(options, reporter)
}
