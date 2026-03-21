use crate::{
    ir::TimingSummary,
    report::{ImplementationReport, StageReport},
};
use anyhow::{Context, Result};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub(crate) struct FlowArtifacts {
    pub(crate) map: PathBuf,
    pub(crate) pack: PathBuf,
    pub(crate) place: PathBuf,
    pub(crate) route: PathBuf,
    pub(crate) device: Option<PathBuf>,
    pub(crate) sta: PathBuf,
    pub(crate) sta_report: PathBuf,
    pub(crate) bitstream: PathBuf,
    pub(crate) bitstream_sidecar: PathBuf,
    pub(crate) report: PathBuf,
}

impl FlowArtifacts {
    pub(crate) fn modern(out_dir: &Path) -> Self {
        Self {
            map: out_dir.join("01-mapped.xml"),
            pack: out_dir.join("02-packed.xml"),
            place: out_dir.join("03-placed.xml"),
            route: out_dir.join("04-routed.xml"),
            device: Some(out_dir.join("04-device.json")),
            sta: out_dir.join("05-timed.xml"),
            sta_report: out_dir.join("05-timing.rpt"),
            bitstream: out_dir.join("06-output.bit"),
            bitstream_sidecar: out_dir.join("06-output.bit.txt"),
            report: out_dir.join("report.json"),
        }
    }

    pub(crate) fn artifact_map(&self) -> BTreeMap<String, String> {
        let mut artifacts = BTreeMap::new();
        artifacts.insert("map".to_string(), self.map.display().to_string());
        artifacts.insert("pack".to_string(), self.pack.display().to_string());
        artifacts.insert("place".to_string(), self.place.display().to_string());
        artifacts.insert("route".to_string(), self.route.display().to_string());
        if let Some(device) = self.device.as_ref().filter(|path| path.exists()) {
            artifacts.insert("device".to_string(), device.display().to_string());
        }
        artifacts.insert("sta".to_string(), self.sta.display().to_string());
        artifacts.insert(
            "sta_report".to_string(),
            self.sta_report.display().to_string(),
        );
        artifacts.insert(
            "bitstream".to_string(),
            self.bitstream.display().to_string(),
        );
        artifacts.insert(
            "bitstream_sidecar".to_string(),
            self.bitstream_sidecar.display().to_string(),
        );
        artifacts.insert("report".to_string(), self.report.display().to_string());
        artifacts
    }
}

pub(crate) fn build_report(
    design: String,
    out_dir: &Path,
    seed: u64,
    artifacts: &FlowArtifacts,
    stages: Vec<StageReport>,
    timing: Option<TimingSummary>,
    bitstream_sha256: Option<String>,
) -> ImplementationReport {
    ImplementationReport {
        design,
        out_dir: out_dir.display().to_string(),
        seed,
        artifacts: artifacts.artifact_map(),
        stages,
        timing,
        bitstream_sha256,
    }
}

pub(crate) fn write_report(path: &Path, report: &ImplementationReport) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(report)?)
        .with_context(|| format!("failed to write {}", path.display()))
}
