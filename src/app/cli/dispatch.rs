use anyhow::Result;
use clap::Parser;

use super::{args::FdeCli, commands::dispatch_command};

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct CliExitError {
    code: i32,
    message: String,
}

impl CliExitError {
    pub(crate) fn timing_violation(message: impl Into<String>) -> Self {
        Self {
            code: 5,
            message: message.into(),
        }
    }

    pub fn code(&self) -> i32 {
        self.code
    }
}

pub fn run() -> Result<()> {
    let cli = FdeCli::parse();
    dispatch_command(cli.command, cli.output)
}
