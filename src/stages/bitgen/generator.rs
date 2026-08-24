use super::{
    api::BitgenOptions, artifacts::prepare_artifacts, circuit::BitgenCircuit,
    payload::build_deterministic_payload, report::build_report, sidecar::build_sidecar,
};
use crate::{ir::BitstreamImage, report::StageOutput};
use anyhow::Result;
use sha2::{Digest, Sha256};

const HEX: &[u8; 16] = b"0123456789abcdef";

pub(super) fn generate_bitstream(
    circuit: &BitgenCircuit,
    options: &BitgenOptions,
) -> Result<StageOutput<BitstreamImage>> {
    let artifacts = prepare_artifacts(circuit, options)?;
    let bytes = match artifacts.text_bitstream.as_ref() {
        Some(serialized) => serialized.text.as_bytes().to_vec(),
        None => build_deterministic_payload(circuit, options, artifacts.config_image.as_ref())?,
    };
    let digest = Sha256::digest(&bytes);
    let mut sha256 = String::with_capacity(digest.len() * 2);
    for byte in digest {
        sha256.push(char::from(HEX[usize::from(byte >> 4)]));
        sha256.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    let sidecar = build_sidecar(circuit, options, &artifacts, &sha256);
    let report = build_report(bytes.len(), &artifacts);

    Ok(StageOutput {
        value: BitstreamImage {
            design_name: circuit.design_name.clone(),
            bytes,
            sidecar_text: sidecar,
            sha256,
        },
        report,
    })
}
