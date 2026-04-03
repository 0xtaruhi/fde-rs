mod emit;
mod encode;
mod layout;
mod model;

use crate::{
    bitgen::ConfigImage,
    cil::Cil,
    resource::{Arch, routing},
};
use anyhow::{Context, Result};
use std::path::Path;

pub use model::SerializedTextBitstream;

#[cfg(test)]
mod decode;

#[cfg(test)]
mod tests;

pub fn serialize_text_bitstream(
    design_name: &str,
    arch: &Arch,
    arch_path: &Path,
    cil: &Cil,
    config_image: &ConfigImage,
) -> Result<Option<SerializedTextBitstream>> {
    if cil.majors.is_empty() || cil.bitstream_commands.is_empty() {
        return Ok(None);
    }

    let mut notes = Vec::new();
    let transmission_defaults = routing::load_site_route_defaults(arch_path, cil)
        .context("failed to load transmission default SRAM state")?;
    let tile_columns =
        layout::build_tile_columns(arch, cil, config_image, &transmission_defaults, &mut notes);
    let major_payloads = encode::build_major_payloads(cil, &tile_columns)
        .context("failed to encode textual major frame payloads")?;
    let memory_payloads =
        encode::build_memory_payloads(cil).context("failed to encode textual memory payloads")?;
    let text = emit::render_bitstream_text(
        design_name,
        &cil.device_name,
        cil,
        &major_payloads,
        &memory_payloads,
        &mut notes,
    )
    .context("failed to render textual bitstream output")?;

    Ok(Some(SerializedTextBitstream {
        text,
        notes,
        major_count: major_payloads.len(),
        memory_count: memory_payloads.len(),
    }))
}
