use anyhow::{Context, Result, anyhow, bail};
use roxmltree::{Document, Node};
use std::{fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SequentialTiming {
    pub clock_to_q_ns: f64,
    pub setup_ns: f64,
}

impl Default for SequentialTiming {
    fn default() -> Self {
        Self {
            clock_to_q_ns: 0.2,
            setup_ns: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CellTimingModel {
    pub sequential: SequentialTiming,
}

pub fn load_cell_timing_model(path: &Path) -> Result<CellTimingModel> {
    let xml = fs::read_to_string(path)
        .with_context(|| format!("failed to read cell timing library {}", path.display()))?;
    let doc = Document::parse(&xml)
        .with_context(|| format!("failed to parse cell timing library {}", path.display()))?;
    let dff = doc
        .descendants()
        .find(|node| node.has_tag_name("cell") && node.attribute("name") == Some("DFF"))
        .ok_or_else(|| anyhow!("cell timing library has no DFF cell"))?;

    let setup_ns = timing_arc_delay(dff, "D", &["setup_rising", "setup_falling"])
        .ok_or_else(|| anyhow!("DFF cell has no setup timing arc on D"))?;
    let clock_to_q_ns = timing_arc_delay(dff, "Q", &["rising_edge", "falling_edge"])
        .ok_or_else(|| anyhow!("DFF cell has no clock-to-Q timing arc on Q"))?;
    for (name, value) in [("setup", setup_ns), ("clock-to-Q", clock_to_q_ns)] {
        if !value.is_finite() || value < 0.0 {
            bail!("DFF {name} delay must be a non-negative finite value");
        }
    }

    Ok(CellTimingModel {
        sequential: SequentialTiming {
            clock_to_q_ns,
            setup_ns,
        },
    })
}

fn timing_arc_delay(cell: Node<'_, '_>, port_name: &str, arc_types: &[&str]) -> Option<f64> {
    let port = cell
        .children()
        .find(|node| node.has_tag_name("port") && node.attribute("name") == Some(port_name))?;
    port.children()
        .filter(|node| node.has_tag_name("timing"))
        .filter(|timing| {
            timing_value(*timing, "timing_type").is_some_and(|kind| arc_types.contains(&kind))
        })
        .filter_map(|timing| {
            ["intrinsic_rise", "intrinsic_fall"]
                .into_iter()
                .filter_map(|name| timing_value(timing, name)?.parse::<f64>().ok())
                .max_by(f64::total_cmp)
        })
        .max_by(f64::total_cmp)
}

fn timing_value<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attribute(name).or_else(|| {
        node.children()
            .find(|child| child.has_tag_name(name))?
            .attribute("value")
    })
}

#[cfg(test)]
mod tests {
    use super::load_cell_timing_model;
    use std::path::PathBuf;

    #[test]
    fn loads_fdp3_dff_setup_and_clock_to_q() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/hw_lib/fdp3_cell.xml");

        let model = load_cell_timing_model(&path).expect("cell timing model");

        assert!((model.sequential.setup_ns - 0.5).abs() < f64::EPSILON);
        assert!((model.sequential.clock_to_q_ns - 1.0).abs() < f64::EPSILON);
    }
}
