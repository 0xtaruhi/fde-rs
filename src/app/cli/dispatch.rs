use anyhow::Result;
use clap::Parser;

use super::{args::FdeCli, commands::dispatch_command};

pub fn run() -> Result<()> {
    let cli = FdeCli::parse();
    dispatch_command(cli.command)
}
