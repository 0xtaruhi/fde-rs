use crate::{
    cil::Cil,
    config_image::{ConfigImage, build_config_image},
    device::DeviceDesign,
    frame_bitstream::{SerializedTextBitstream, serialize_text_bitstream},
    ir::{BitstreamImage, Cluster, Design, Net},
    report::{StageOutput, StageReport},
    resource::{Arch, load_arch},
    route_bits::{DeviceRouteImage, route_device_design},
};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{fmt::Write, path::PathBuf};

#[derive(Debug, Clone, Default)]
pub struct BitgenOptions {
    pub arch_name: Option<String>,
    pub arch_path: Option<PathBuf>,
    pub cil_path: Option<PathBuf>,
    pub cil: Option<Cil>,
    pub device_design: Option<DeviceDesign>,
}

struct PreparedArtifacts {
    route_image: Option<DeviceRouteImage>,
    config_image: Option<ConfigImage>,
    text_bitstream: Option<SerializedTextBitstream>,
}

pub fn run(design: Design, options: &BitgenOptions) -> Result<StageOutput<BitstreamImage>> {
    let artifacts = prepare_artifacts(&design, options)?;
    let clusters = sorted_clusters(&design);
    let nets = sorted_nets(&design);

    let bytes = match artifacts.text_bitstream.as_ref() {
        Some(serialized) => serialized.text.as_bytes().to_vec(),
        None => build_deterministic_payload(
            &design,
            options,
            &clusters,
            &nets,
            artifacts.config_image.as_ref(),
        ),
    };
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let sidecar = build_sidecar(&design, options, &clusters, &nets, &artifacts, &sha256);
    let report = build_report(bytes.len(), &artifacts);

    Ok(StageOutput {
        value: BitstreamImage {
            design_name: design.name,
            bytes,
            sidecar_text: sidecar,
            sha256,
        },
        report,
    })
}

fn prepare_artifacts(design: &Design, options: &BitgenOptions) -> Result<PreparedArtifacts> {
    let arch = load_optional_arch(options)?;
    let route_image = build_route_image(options, arch.as_ref())?;
    let config_image = build_config_image_if_available(options, route_image.as_ref())?;
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
    route_image: Option<&DeviceRouteImage>,
) -> Result<Option<ConfigImage>> {
    match (options.device_design.as_ref(), options.cil.as_ref()) {
        (Some(device_design), Some(cil)) => build_config_image(device_design, cil, route_image)
            .context("failed to build tile configuration image")
            .map(Some),
        _ => Ok(None),
    }
}

fn build_text_bitstream(
    design: &Design,
    options: &BitgenOptions,
    arch: Option<&Arch>,
    config_image: Option<&ConfigImage>,
) -> Result<Option<SerializedTextBitstream>> {
    match (arch, options.cil.as_ref(), config_image) {
        (Some(arch), Some(cil), Some(config_image)) => {
            serialize_text_bitstream(&design.name, arch, cil, config_image)
                .context("failed to serialize textual bitstream")
        }
        _ => Ok(None),
    }
}

fn sorted_clusters(design: &Design) -> Vec<Cluster> {
    let mut clusters = design.clusters.clone();
    clusters.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    clusters
}

fn sorted_nets(design: &Design) -> Vec<Net> {
    let mut nets = design.nets.clone();
    nets.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    nets
}

fn build_sidecar(
    design: &Design,
    options: &BitgenOptions,
    clusters: &[Cluster],
    nets: &[Net],
    artifacts: &PreparedArtifacts,
    sha256: &str,
) -> String {
    let mut sidecar = String::new();
    let _ = writeln!(sidecar, "# FDE bitstream sidecar");
    let _ = writeln!(sidecar, "design={}", design.name);
    let _ = writeln!(sidecar, "stage={}", design.stage);
    let _ = writeln!(
        sidecar,
        "mode={}",
        if artifacts.text_bitstream.is_some() {
            "text-bitstream"
        } else {
            "deterministic-payload"
        }
    );
    if let Some(arch_name) = options.arch_name.as_ref() {
        let _ = writeln!(sidecar, "arch={}", arch_name);
    }
    if let Some(cil_path) = options.cil_path.as_ref() {
        let _ = writeln!(sidecar, "cil={}", cil_path.display());
    }
    let _ = writeln!(sidecar, "sha256={}", sha256);
    let _ = writeln!(sidecar);
    for cluster in clusters {
        let _ = writeln!(
            sidecar,
            "CLUSTER {} @ {},{} :: {}",
            cluster.name,
            cluster.x.unwrap_or(0),
            cluster.y.unwrap_or(0),
            cluster.members.join(",")
        );
    }
    for net in nets {
        let _ = writeln!(
            sidecar,
            "NET {} len={} route={}",
            net.name,
            net.route_length(),
            net.route
                .iter()
                .map(|segment| format!(
                    "{}:{}-{}:{}",
                    segment.x0, segment.y0, segment.x1, segment.y1
                ))
                .collect::<Vec<_>>()
                .join("|")
        );
    }
    if let Some(serialized) = artifacts.text_bitstream.as_ref() {
        render_bitstream_sidecar(&mut sidecar, serialized);
    }
    if let Some(config_image) = artifacts.config_image.as_ref() {
        render_config_image_sidecar(&mut sidecar, config_image);
    }
    if let Some(route_image) = artifacts.route_image.as_ref() {
        render_route_sidecar(&mut sidecar, route_image);
    }
    sidecar
}

fn build_report(byte_count: usize, artifacts: &PreparedArtifacts) -> StageReport {
    let mut report = StageReport::new("bitgen");
    if let Some(serialized) = artifacts.text_bitstream.as_ref() {
        report.push(format!(
            "Generated {} bytes of textual bitstream across {} major chunks and {} memory chunks.",
            byte_count, serialized.major_count, serialized.memory_count
        ));
    } else {
        report.push(format!(
            "Generated {} bytes of deterministic bitstream payload.",
            byte_count
        ));
    }
    if let Some(config_image) = artifacts.config_image.as_ref() {
        let set_bits = config_image
            .tiles
            .iter()
            .map(|tile| tile.set_bit_count())
            .sum::<usize>();
        let routed_pips = artifacts
            .route_image
            .as_ref()
            .map(|image| image.pips.len())
            .unwrap_or(0);
        report.push(format!(
            "Materialized {} config bits across {} tile images with {} routed pips.",
            set_bits,
            config_image.tiles.len(),
            routed_pips
        ));
    }
    report
}

fn build_deterministic_payload(
    design: &Design,
    options: &BitgenOptions,
    clusters: &[crate::ir::Cluster],
    nets: &[crate::ir::Net],
    config_image: Option<&ConfigImage>,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"FDEBIT24");
    write_chunk(&mut bytes, "design", design.name.as_bytes());
    write_chunk(&mut bytes, "stage", design.stage.as_bytes());
    if let Some(arch_name) = options.arch_name.as_ref() {
        write_chunk(&mut bytes, "arch", arch_name.as_bytes());
    }
    if let Some(cil_path) = options.cil_path.as_ref() {
        write_chunk(&mut bytes, "cil", cil_path.to_string_lossy().as_bytes());
    }

    for cluster in clusters {
        let payload = format!(
            "{}@{},{}:{}",
            cluster.name,
            cluster.x.unwrap_or(0),
            cluster.y.unwrap_or(0),
            cluster.members.join(",")
        );
        write_chunk(&mut bytes, "clb", payload.as_bytes());
    }

    for net in nets {
        let payload = format!(
            "{}:{}:{}",
            net.name,
            net.route_length(),
            net.route
                .iter()
                .map(|segment| format!(
                    "{}:{}-{}:{}",
                    segment.x0, segment.y0, segment.x1, segment.y1
                ))
                .collect::<Vec<_>>()
                .join("|")
        );
        write_chunk(&mut bytes, "net", payload.as_bytes());
    }

    if let Some(config_image) = config_image {
        append_config_image_chunks(&mut bytes, config_image);
    }

    let digest = Sha256::digest(&bytes);
    bytes.extend_from_slice(&digest);
    bytes
}

fn append_config_image_chunks(bytes: &mut Vec<u8>, config_image: &ConfigImage) {
    for tile in &config_image.tiles {
        let header = format!(
            "{}:{}@{},{}:{}x{}",
            tile.tile_name, tile.tile_type, tile.x, tile.y, tile.rows, tile.cols
        );
        let mut payload = Vec::new();
        payload.extend_from_slice(header.as_bytes());
        payload.push(0);
        payload.extend_from_slice(&tile.packed_bits());
        write_chunk(bytes, "tile", &payload);
    }
}

fn render_bitstream_sidecar(sidecar: &mut String, serialized: &SerializedTextBitstream) {
    let _ = writeln!(sidecar);
    let _ = writeln!(sidecar, "# Text Bitstream");
    let _ = writeln!(sidecar, "MAJORS {}", serialized.major_count);
    let _ = writeln!(sidecar, "MEMORIES {}", serialized.memory_count);
    for note in &serialized.notes {
        let _ = writeln!(sidecar, "NOTE {}", note);
    }
}

fn render_config_image_sidecar(sidecar: &mut String, config_image: &ConfigImage) {
    let _ = writeln!(sidecar);
    let _ = writeln!(sidecar, "# Tile Config Image");
    for note in &config_image.notes {
        let _ = writeln!(sidecar, "NOTE {}", note);
    }
    for tile in &config_image.tiles {
        let _ = writeln!(
            sidecar,
            "TILE {} type={} @ {},{} set_bits={} packed_bytes={}",
            tile.tile_name,
            tile.tile_type,
            tile.x,
            tile.y,
            tile.set_bit_count(),
            tile.packed_bits().len()
        );
        for cfg in &tile.configs {
            let _ = writeln!(
                sidecar,
                "CFG {} {}={}",
                cfg.site_name, cfg.cfg_name, cfg.function_name
            );
        }
        for bit in tile.assignments.iter().filter(|bit| bit.value != 0) {
            let _ = writeln!(
                sidecar,
                "BIT {} {} {} {}:{} -> B{}W{}",
                bit.site_name,
                bit.cfg_name,
                bit.function_name,
                bit.basic_cell,
                bit.sram_name,
                bit.row,
                bit.col
            );
        }
    }
}

fn render_route_sidecar(sidecar: &mut String, route_image: &DeviceRouteImage) {
    let _ = writeln!(sidecar);
    let _ = writeln!(sidecar, "# Routed Transmission Pips");
    for pip in &route_image.pips {
        let _ = writeln!(
            sidecar,
            "PIP {} {} {} @ {},{} {} -> {}",
            pip.net_name, pip.tile_name, pip.site_name, pip.x, pip.y, pip.from_net, pip.to_net
        );
    }
}

fn write_chunk(bytes: &mut Vec<u8>, tag: &str, payload: &[u8]) {
    bytes.extend_from_slice(&(tag.len() as u32).to_le_bytes());
    bytes.extend_from_slice(tag.as_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
}

#[cfg(test)]
mod tests {
    use super::{BitgenOptions, run};
    use crate::ir::{BitstreamImage, Cluster, Design, Net, RouteSegment};
    use anyhow::Result;

    fn routed_design() -> Design {
        Design {
            name: "bitgen-mini".to_string(),
            stage: "timed".to_string(),
            clusters: vec![Cluster {
                name: "clb0".to_string(),
                kind: "logic".to_string(),
                members: vec!["u0".to_string()],
                capacity: 1,
                x: Some(1),
                y: Some(1),
                ..Cluster::default()
            }],
            nets: vec![Net {
                name: "mid".to_string(),
                route: vec![
                    RouteSegment {
                        x0: 0,
                        y0: 1,
                        x1: 1,
                        y1: 1,
                    },
                    RouteSegment {
                        x0: 1,
                        y0: 1,
                        x1: 2,
                        y1: 1,
                    },
                ],
                ..Net::default()
            }],
            ..Design::default()
        }
    }

    fn assert_image(image: &BitstreamImage) {
        assert!(image.bytes.starts_with(b"FDEBIT24"));
        assert_eq!(image.design_name, "bitgen-mini");
        assert_eq!(image.sha256.len(), 64);
        assert!(image.sidecar_text.contains("mode=deterministic-payload"));
        assert!(image.sidecar_text.contains("CLUSTER clb0"));
        assert!(image.sidecar_text.contains("NET mid len=2"));
    }

    #[test]
    fn falls_back_to_deterministic_payload_without_resources() -> Result<()> {
        let result = run(routed_design(), &BitgenOptions::default())?;
        assert_image(&result.value);
        assert!(
            result
                .report
                .messages
                .iter()
                .any(|message| { message.contains("deterministic bitstream payload") })
        );
        Ok(())
    }
}
