use super::{
    api::BitgenOptions, artifacts::prepare_artifacts, circuit::BitgenCircuit,
    payload::build_deterministic_payload, report::build_report, sidecar::build_sidecar,
};
use crate::{ir::BitstreamImage, report::StageOutput};
use anyhow::Result;
use sha2::{Digest, Sha256};

pub(super) fn generate_bitstream(
    circuit: &BitgenCircuit,
    options: &BitgenOptions,
) -> Result<StageOutput<BitstreamImage>> {
    let artifacts = prepare_artifacts(circuit, options)?;
    let bytes = match artifacts.text_bitstream.as_ref() {
        Some(serialized) => serialized.text.as_bytes().to_vec(),
        None => build_deterministic_payload(circuit, options, artifacts.config_image.as_ref()),
    };
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
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
