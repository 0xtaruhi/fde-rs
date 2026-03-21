use super::{
    DeviceDesign, artifacts::prepare_artifacts, payload::build_deterministic_payload,
    report::build_report, sidecar::build_sidecar,
};
use crate::{
    cil::Cil,
    ir::{BitstreamImage, Cluster, Design, Net},
    report::StageOutput,
};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct BitgenOptions {
    pub arch_name: Option<String>,
    pub arch_path: Option<PathBuf>,
    pub cil_path: Option<PathBuf>,
    pub cil: Option<Cil>,
    pub device_design: Option<DeviceDesign>,
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
