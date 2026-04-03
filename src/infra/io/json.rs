use crate::ir::Design;
use anyhow::{Context, Result};
use std::path::Path;

pub(super) fn load_design_json(text: &str, path: &Path) -> Result<Design> {
    serde_json::from_str(text)
        .with_context(|| format!("failed to parse json design {}", path.display()))
}

pub(super) fn save_design_json(design: &Design) -> Result<String> {
    serde_json::to_string_pretty(design).context("failed to serialize design json")
}
