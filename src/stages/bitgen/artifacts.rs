use super::{
    ConfigImage, ProgrammingImage, SerializedTextBitstream, api::BitgenOptions,
    build_programming_image, circuit::BitgenCircuit, config_image::encode_config_image,
    serialize_text_bitstream,
};
use crate::resource::{Arch, load_arch};
use anyhow::{Context, Result};

pub(super) struct PreparedArtifacts {
    pub(super) programming_image: Option<ProgrammingImage>,
    pub(super) config_image: Option<ConfigImage>,
    pub(super) text_bitstream: Option<SerializedTextBitstream>,
}

pub(super) fn prepare_artifacts(
    circuit: &BitgenCircuit,
    options: &BitgenOptions,
) -> Result<PreparedArtifacts> {
    let arch = load_optional_arch(options)?;
    let programming_image = prepare_architecture_backed_programming(options)?;
    let config_image = build_config_image_if_available(
        programming_image.as_ref(),
        options.cil.as_ref(),
        arch.as_ref(),
    )?;
    let text_bitstream =
        build_text_bitstream(circuit, options, arch.as_ref(), config_image.as_ref())?;

    Ok(PreparedArtifacts {
        programming_image,
        config_image,
        text_bitstream,
    })
}

fn load_optional_arch(options: &BitgenOptions) -> Result<Option<Arch>> {
    options
        .arch_path
        .as_deref()
        .map(|path| {
            load_arch(path)
                .with_context(|| format!("failed to load bitgen architecture {}", path.display()))
        })
        .transpose()
}

fn prepare_architecture_backed_programming(
    options: &BitgenOptions,
) -> Result<Option<ProgrammingImage>> {
    if let (Some(device_design), Some(cil)) = (options.device_design.as_ref(), options.cil.as_ref())
    {
        return Ok(Some(build_programming_image(
            device_design,
            cil,
            options.route_image.as_ref(),
        )));
    }

    Ok(None)
}

fn build_config_image_if_available(
    programming_image: Option<&ProgrammingImage>,
    cil: Option<&crate::cil::Cil>,
    arch: Option<&Arch>,
) -> Result<Option<ConfigImage>> {
    match (programming_image, cil) {
        (Some(programming_image), Some(cil)) => encode_config_image(programming_image, cil, arch)
            .context("failed to build tile configuration image")
            .map(Some),
        _ => Ok(None),
    }
}

fn build_text_bitstream(
    circuit: &BitgenCircuit,
    options: &BitgenOptions,
    arch: Option<&Arch>,
    config_image: Option<&ConfigImage>,
) -> Result<Option<SerializedTextBitstream>> {
    match (
        arch,
        options.arch_path.as_deref(),
        options.cil.as_ref(),
        config_image,
    ) {
        (Some(arch), Some(arch_path), Some(cil), Some(config_image)) => {
            serialize_text_bitstream(&circuit.design_name, arch, arch_path, cil, config_image)
                .context("failed to serialize textual bitstream")
        }
        _ => Ok(None),
    }
}
