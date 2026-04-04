use super::artifacts::PreparedArtifacts;
use crate::report::StageReport;

pub(super) fn build_report(byte_count: usize, artifacts: &PreparedArtifacts) -> StageReport {
    let mut report = StageReport::new("bitgen");
    if let Some(serialized) = artifacts.text_bitstream.as_ref() {
        report.metric("byte_count", byte_count);
        report.metric("major_chunk_count", serialized.major_count);
        report.metric("memory_chunk_count", serialized.memory_count);
        report.push(format!(
            "Generated {} bytes of textual bitstream across {} major chunks and {} memory chunks.",
            byte_count, serialized.major_count, serialized.memory_count
        ));
    } else {
        report.metric("byte_count", byte_count);
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
            .programming_image
            .as_ref()
            .map(|image| image.routes.len())
            .unwrap_or(0);
        report.metric("config_bit_count", set_bits);
        report.metric("tile_image_count", config_image.tiles.len());
        report.metric("routed_pip_count", routed_pips);
        report.push(format!(
            "Materialized {} config bits across {} tile images with {} routed pips.",
            set_bits,
            config_image.tiles.len(),
            routed_pips
        ));
    }
    report
}
