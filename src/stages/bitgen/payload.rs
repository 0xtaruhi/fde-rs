use super::{ConfigImage, api::BitgenOptions};
use crate::ir::{Cluster, Design, Net};
use sha2::{Digest, Sha256};

pub(super) fn build_deterministic_payload(
    design: &Design,
    options: &BitgenOptions,
    clusters: &[Cluster],
    nets: &[Net],
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

fn write_chunk(bytes: &mut Vec<u8>, tag: &str, payload: &[u8]) {
    bytes.extend_from_slice(&(tag.len() as u32).to_le_bytes());
    bytes.extend_from_slice(tag.as_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
}
