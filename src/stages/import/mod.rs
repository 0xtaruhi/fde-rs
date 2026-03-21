use crate::{
    edif::load_edif,
    io::load_design,
    ir::Design,
    report::{StageOutput, StageReport},
};
use anyhow::{Result, bail};
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    pub source_hint: Option<String>,
}

pub fn run_path(input: &Path, options: &ImportOptions) -> Result<StageOutput<Design>> {
    let mut design = match input
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "edf" | "edif" => load_edif(input)?,
        "xml" | "json" => load_design(input)?,
        "v" | "sv" => {
            bail!(
                "Verilog import is intentionally unsupported in this Rust rewrite. Synthesize with Yosys first and pass the EDIF to `fde map` or `fde impl`."
            )
        }
        _ => bail!("unsupported import format for {}", input.display()),
    };

    design.stage = "imported".to_string();
    design.note(format!("Imported from {}", input.display()));
    if let Some(hint) = &options.source_hint {
        design.note(format!("source_hint={hint}"));
    }

    let mut report = StageReport::new("import");
    report.push(format!(
        "Imported {} cells and {} nets from {}.",
        design.cells.len(),
        design.nets.len(),
        input.display()
    ));

    Ok(StageOutput {
        value: design,
        report,
    })
}
