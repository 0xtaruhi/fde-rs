mod modern;
mod options;
mod report;
mod resources;

use crate::report::ImplementationReport;
use anyhow::Result;

pub use options::ImplementationOptions;

pub fn run(options: &ImplementationOptions) -> Result<ImplementationReport> {
    modern::run(options)
}
