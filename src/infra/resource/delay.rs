use anyhow::{Context, Result};
use roxmltree::Document;
use std::{fs, path::Path};

/// Intrinsic per-cell-type delays, overridable from the delay XML so timing
/// models stay data-driven. Defaults reproduce the historical hard-coded
/// estimates exactly.
#[derive(Debug, Clone, Copy)]
pub struct CellIntrinsicDelays {
    pub lut_base_ns: f64,
    pub lut_per_input_ns: f64,
    pub buffer_delay_ns: f64,
    pub other_base_ns: f64,
    pub other_per_input_ns: f64,
}

impl Default for CellIntrinsicDelays {
    fn default() -> Self {
        Self {
            lut_base_ns: 0.15,
            lut_per_input_ns: 0.04,
            buffer_delay_ns: 0.04,
            other_base_ns: 0.08,
            other_per_input_ns: 0.02,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DelayModel {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub values: Vec<Vec<f64>>,
    pub cell_delays: CellIntrinsicDelays,
}

pub fn load_delay_model(path: Option<&Path>) -> Result<Option<DelayModel>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let xml = fs::read_to_string(path)
        .with_context(|| format!("failed to read delay model {}", path.display()))?;
    let doc = Document::parse(&xml)
        .with_context(|| format!("failed to parse delay model {}", path.display()))?;
    let Some(table) = doc
        .descendants()
        .find(|node| node.has_tag_name("table") && node.attribute("name") == Some("clb2clb"))
    else {
        return Ok(None);
    };

    let (height, width) = table
        .attribute("scale")
        .and_then(parse_point)
        .unwrap_or((0, 0));
    let mut values = Vec::new();
    for line in table
        .text()
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let row = line
            .split_whitespace()
            .filter_map(|value| value.parse::<f64>().ok())
            .collect::<Vec<_>>();
        if !row.is_empty() {
            values.push(row);
        }
    }
    if values.is_empty() {
        return Ok(None);
    }

    Ok(Some(DelayModel {
        name: "clb2clb".to_string(),
        width,
        height,
        values,
        cell_delays: parse_cell_delays(&doc),
    }))
}

/// Optional `<cell_delays>` overrides; anything absent keeps the default.
fn parse_cell_delays(doc: &Document) -> CellIntrinsicDelays {
    let mut delays = CellIntrinsicDelays::default();
    let Some(element) = doc
        .descendants()
        .find(|node| node.has_tag_name("cell_delays"))
    else {
        return delays;
    };
    for node in element.children().filter(roxmltree::Node::is_element) {
        let attribute = |name: &str| node.attribute(name).and_then(|v| v.parse::<f64>().ok());
        match node.tag_name().name() {
            "lut" => {
                if let Some(v) = attribute("base") {
                    delays.lut_base_ns = v;
                }
                if let Some(v) = attribute("per_input") {
                    delays.lut_per_input_ns = v;
                }
            }
            "buffer" => {
                if let Some(v) = attribute("delay") {
                    delays.buffer_delay_ns = v;
                }
            }
            "other" => {
                if let Some(v) = attribute("base") {
                    delays.other_base_ns = v;
                }
                if let Some(v) = attribute("per_input") {
                    delays.other_per_input_ns = v;
                }
            }
            _ => {}
        }
    }
    delays
}

impl DelayModel {
    pub fn lookup(&self, dx: usize, dy: usize) -> f64 {
        if self.values.is_empty() {
            return 0.1 * (dx + dy) as f64;
        }
        let row = dy.min(self.values.len().saturating_sub(1));
        let col = dx.min(self.values[row].len().saturating_sub(1));
        self.values[row][col]
    }
}

fn parse_point(raw: &str) -> Option<(usize, usize)> {
    let mut parts = raw.split(',').map(str::trim);
    let x = parts.next()?.parse().ok()?;
    let y = parts.next()?.parse().ok()?;
    Some((x, y))
}

#[cfg(test)]
mod tests {
    use super::load_delay_model;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn write_model(xml: &str) -> (NamedTempFile, PathBuf) {
        let file = NamedTempFile::new().expect("tempfile");
        std::fs::write(file.path(), xml).expect("write delay xml");
        let path = file.path().to_path_buf();
        (file, path)
    }

    #[test]
    fn loads_clb2clb_table_and_keeps_default_cell_delays() {
        let (_file, path) = write_model(
            "<plc_delay><table name=\"clb2clb\" scale=\"2,2\">0.1 0.2\n0.3 0.4</table></plc_delay>",
        );
        let model = load_delay_model(Some(&path)).expect("load").expect("model");
        assert_eq!(model.name, "clb2clb");
        assert!((model.lookup(1, 1) - 0.4).abs() < f64::EPSILON);
        let delays = model.cell_delays;
        assert!((delays.lut_base_ns - 0.15).abs() < 1e-9);
    }

    #[test]
    fn parses_optional_cell_delay_overrides() {
        let (_file, path) = write_model(
            "<plc_delay><table name=\"clb2clb\" scale=\"1,1\">0.5</table>\
             <cell_delays>\
               <lut base=\"0.3\" per_input=\"0.01\"/>\
               <buffer delay=\"0.02\"/>\
               <ff delay=\"0.2\"/>\
               <other base=\"0.05\" per_input=\"0.03\"/>\
             </cell_delays></plc_delay>",
        );
        let model = load_delay_model(Some(&path)).expect("load").expect("model");
        let delays = model.cell_delays;
        assert!((delays.lut_base_ns - 0.3).abs() < 1e-9);
        assert!((delays.lut_per_input_ns - 0.01).abs() < 1e-9);
        assert!((delays.buffer_delay_ns - 0.02).abs() < 1e-9);
        assert!((delays.other_base_ns - 0.05).abs() < 1e-9);
        assert!((delays.other_per_input_ns - 0.03).abs() < 1e-9);
    }

    #[test]
    fn missing_file_yields_none_without_panic() {
        let path = PathBuf::from("/nonexistent/fdp3p7_dly.xml");
        assert!(matches!(load_delay_model(Some(&path)), Ok(None) | Err(_)));
    }
}
