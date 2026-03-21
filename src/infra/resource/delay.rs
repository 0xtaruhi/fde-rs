use anyhow::{Context, Result};
use roxmltree::Document;
use std::{fs, path::Path};

#[derive(Debug, Clone, Default)]
pub struct DelayModel {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub values: Vec<Vec<f64>>,
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
    }))
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
