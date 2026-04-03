mod json;
mod xml;

use crate::{cil::Cil, constraints::ConstraintEntry, ir::Design, resource::Arch};
use anyhow::{Context, Result};
use std::{fs, path::Path};

pub fn load_design(path: &Path) -> Result<Design> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read design {}", path.display()))?;
    match file_extension(path).as_str() {
        "json" => json::load_design_json(&text, path),
        _ => xml::load_design_xml(&text),
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DesignWriteContext<'a> {
    pub arch: Option<&'a Arch>,
    pub cil: Option<&'a Cil>,
    pub constraints: &'a [ConstraintEntry],
    pub cil_path: Option<&'a Path>,
}

pub fn save_design(design: &Design, path: &Path) -> Result<()> {
    save_design_with_context(design, path, &DesignWriteContext::default())
}

pub fn save_design_with_context(
    design: &Design,
    path: &Path,
    context: &DesignWriteContext<'_>,
) -> Result<()> {
    let data = match file_extension(path).as_str() {
        "json" => json::save_design_json(design)?,
        _ => xml::save_fde_design_xml_with_context(design, context)?,
    };
    fs::write(path, data).with_context(|| format!("failed to write design {}", path.display()))
}

fn file_extension(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}
