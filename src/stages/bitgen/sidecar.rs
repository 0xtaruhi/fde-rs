use super::{
    ConfigImage, SerializedTextBitstream, api::BitgenOptions, artifacts::PreparedArtifacts,
    circuit::BitgenCircuit,
};
use crate::bitgen::ProgrammingImage;
use std::fmt::Write;

pub(super) fn build_sidecar(
    circuit: &BitgenCircuit,
    options: &BitgenOptions,
    artifacts: &PreparedArtifacts,
    sha256: &str,
) -> String {
    let mut sidecar = String::new();
    let _ = writeln!(sidecar, "# FDE bitstream sidecar");
    let _ = writeln!(sidecar, "design={}", circuit.design_name);
    let _ = writeln!(sidecar, "stage={}", circuit.stage_name);
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
    for cluster in &circuit.clusters {
        let _ = writeln!(
            sidecar,
            "CLUSTER {} @ {},{} :: {}",
            cluster.name,
            cluster.x.unwrap_or(0),
            cluster.y.unwrap_or(0),
            cluster.members.join(",")
        );
    }
    for net in &circuit.nets {
        let _ = writeln!(
            sidecar,
            "NET {} len={} route={}",
            net.name,
            net.route_length(),
            serialize_net_route(net)
        );
    }
    if let Some(serialized) = artifacts.text_bitstream.as_ref() {
        render_bitstream_sidecar(&mut sidecar, serialized);
    }
    if let Some(config_image) = artifacts.config_image.as_ref() {
        render_config_image_sidecar(&mut sidecar, config_image);
    }
    if let Some(programming_image) = artifacts.programming_image.as_ref() {
        render_programming_sidecar(&mut sidecar, programming_image);
    }
    sidecar
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

fn render_programming_sidecar(sidecar: &mut String, programming: &ProgrammingImage) {
    let _ = writeln!(sidecar);
    let _ = writeln!(sidecar, "# Routed Transmission Pips");
    for note in &programming.notes {
        let _ = writeln!(sidecar, "NOTE {}", note);
    }
    for pip in &programming.routes {
        let _ = writeln!(
            sidecar,
            "PIP {} {} {} @ {},{} {} -> {}",
            pip.net_name, pip.tile_name, pip.site_name, pip.x, pip.y, pip.from_net, pip.to_net
        );
    }
}

fn serialize_net_route(net: &crate::ir::Net) -> String {
    if !net.route.is_empty() {
        net.route
            .iter()
            .map(|segment| {
                format!(
                    "{}:{}-{}:{}",
                    segment.x0, segment.y0, segment.x1, segment.y1
                )
            })
            .collect::<Vec<_>>()
            .join("|")
    } else {
        net.route_pips
            .iter()
            .map(|pip| format!("{}:{}@{},{}", pip.from_net, pip.to_net, pip.x, pip.y))
            .collect::<Vec<_>>()
            .join("|")
    }
}
