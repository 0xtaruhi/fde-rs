use crate::{edif::load_edif, io::load_design, ir::Design};
use anyhow::{Result, bail};
use std::path::Path;

pub fn load_input(path: &Path) -> Result<Design> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "edf" | "edif" => load_edif(path),
        "xml" | "json" => load_design(path),
        "v" | "sv" => bail!(
            "Verilog is not a primary frontend in this rewrite. Use Yosys to generate EDIF first."
        ),
        _ => bail!("unsupported map input format for {}", path.display()),
    }
}
