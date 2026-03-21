use super::{
    ConfigImage, DeviceRouteImage, SerializedTextBitstream, api::BitgenOptions, build_config_image,
    route_device_design, serialize_text_bitstream,
};
use crate::{
    ir::Design,
    resource::{Arch, load_arch},
};
use anyhow::{Context, Result};

pub(super) struct PreparedArtifacts {
    pub(super) route_image: Option<DeviceRouteImage>,
    pub(super) config_image: Option<ConfigImage>,
    pub(super) text_bitstream: Option<SerializedTextBitstream>,
}

pub(super) fn prepare_artifacts(
    design: &Design,
    options: &BitgenOptions,
) -> Result<PreparedArtifacts> {
    let arch = load_optional_arch(options)?;
    let route_image = build_route_image(options, arch.as_ref())?;
    let config_image =
        build_config_image_if_available(options, arch.as_ref(), route_image.as_ref())?;
    let text_bitstream =
        build_text_bitstream(design, options, arch.as_ref(), config_image.as_ref())?;

    Ok(PreparedArtifacts {
        route_image,
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

fn build_route_image(
    options: &BitgenOptions,
    arch: Option<&Arch>,
) -> Result<Option<DeviceRouteImage>> {
    match (
        options.device_design.as_ref(),
        options.cil.as_ref(),
        options.arch_path.as_deref(),
        arch,
    ) {
        (Some(device_design), Some(cil), Some(arch_path), Some(arch)) => {
            route_device_design(device_design, arch, arch_path, cil)
                .with_context(|| {
                    format!(
                        "failed to derive routed transmission bits from architecture {}",
                        arch_path.display()
                    )
                })
                .map(Some)
        }
        _ => Ok(None),
    }
}

fn build_config_image_if_available(
    options: &BitgenOptions,
    arch: Option<&Arch>,
    route_image: Option<&DeviceRouteImage>,
) -> Result<Option<ConfigImage>> {
    match (options.device_design.as_ref(), options.cil.as_ref()) {
        (Some(device_design), Some(cil)) => {
            build_config_image(device_design, cil, arch, route_image)
                .context("failed to build tile configuration image")
                .map(Some)
        }
        _ => Ok(None),
    }
}

fn build_text_bitstream(
    design: &Design,
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
            serialize_text_bitstream(&design.name, arch, arch_path, cil, config_image)
                .context("failed to serialize textual bitstream")
        }
        _ => Ok(None),
    }
}
