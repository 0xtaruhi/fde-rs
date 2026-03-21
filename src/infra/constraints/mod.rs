use crate::{ir::Design, resource::Arch};
use anyhow::{Context, Result, bail};
use roxmltree::Document;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path, sync::Arc};

pub type SharedConstraints = Arc<[ConstraintEntry]>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConstraintEntry {
    pub port_name: String,
    pub pin_name: String,
}

pub fn load_constraints(path: &Path) -> Result<Vec<ConstraintEntry>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read constraint file {}", path.display()))?;
    let doc = Document::parse(&text)
        .with_context(|| format!("failed to parse constraint file {}", path.display()))?;
    let mut entries = Vec::new();
    for node in doc.descendants().filter(|node| node.has_tag_name("port")) {
        let Some(name) = node.attribute("name") else {
            continue;
        };
        let Some(position) = node.attribute("position") else {
            continue;
        };
        entries.push(ConstraintEntry {
            port_name: name.to_string(),
            pin_name: position.to_string(),
        });
    }
    Ok(entries)
}

pub fn apply_constraint_file(
    design: &mut Design,
    arch: &Arch,
    path: Option<&Path>,
) -> Result<Vec<ConstraintEntry>> {
    let constraints = match path {
        Some(path) => load_constraints(path)?,
        None => Vec::new(),
    };
    apply_constraints(design, arch, &constraints);
    Ok(constraints)
}

pub fn apply_constraints(design: &mut Design, arch: &Arch, constraints: &[ConstraintEntry]) {
    for constraint in constraints {
        if let Some(port) = design
            .ports
            .iter_mut()
            .find(|port| port.name == constraint.port_name)
        {
            port.pin = Some(constraint.pin_name.clone());
            if let Some((x, y)) = arch.pad_lookup.get(&constraint.pin_name) {
                port.x = Some(*x);
                port.y = Some(*y);
            }
        }
    }
}

pub fn ensure_port_positions(design: &mut Design, arch: &Arch) {
    for (index, port) in design.ports.iter_mut().enumerate() {
        if port.x.is_some() && port.y.is_some() {
            continue;
        }
        if let Some(pin) = port.pin.as_deref()
            && let Some((x, y)) = arch.pad_lookup.get(pin)
        {
            port.x = Some(*x);
            port.y = Some(*y);
            continue;
        }
        let (x, y) = arch.fallback_port_position(index, port.direction.is_input_like());
        port.x = Some(x);
        port.y = Some(y);
    }
}

pub fn ensure_cluster_positions(design: &Design) -> Result<()> {
    for cluster in &design.clusters {
        if cluster.x.is_none() || cluster.y.is_none() {
            bail!("cluster {} is missing placement coordinates", cluster.name);
        }
    }
    Ok(())
}
