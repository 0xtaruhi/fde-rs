use crate::{ir::Design, resource::Arch};
use anyhow::{Context, Result, anyhow, bail};
use roxmltree::Document;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path, sync::Arc};

pub type SharedConstraints = Arc<[ConstraintEntry]>;
pub type SharedClockConstraints = Arc<[ClockConstraint]>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConstraintEntry {
    pub port_name: String,
    pub pin_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClockConstraint {
    pub name: String,
    pub port_name: String,
    pub period_ns: f64,
}

#[derive(Debug, Clone, Default)]
pub struct ConstraintSet {
    pub pins: Vec<ConstraintEntry>,
    pub clocks: Vec<ClockConstraint>,
}

pub fn load_constraints(path: &Path) -> Result<Vec<ConstraintEntry>> {
    Ok(load_constraint_set(path)?.pins)
}

pub fn load_constraint_set(path: &Path) -> Result<ConstraintSet> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read constraint file {}", path.display()))?;
    let doc = Document::parse(&text)
        .with_context(|| format!("failed to parse constraint file {}", path.display()))?;
    let mut pins = Vec::new();
    for node in doc.descendants().filter(|node| node.has_tag_name("port")) {
        let Some(name) = node.attribute("name") else {
            continue;
        };
        let Some(position) = node.attribute("position") else {
            continue;
        };
        pins.push(ConstraintEntry {
            port_name: name.to_string(),
            pin_name: position.to_string(),
        });
    }

    let clocks = parse_clock_constraints(&doc)?;
    Ok(ConstraintSet { pins, clocks })
}

fn parse_clock_constraints(doc: &Document<'_>) -> Result<Vec<ClockConstraint>> {
    let mut clocks = Vec::new();
    let mut names = BTreeSet::new();
    let mut ports = BTreeSet::new();
    for node in doc.descendants().filter(|node| node.has_tag_name("clock")) {
        let port_name = node
            .attribute("port")
            .ok_or_else(|| anyhow!("clock constraint is missing its 'port' attribute"))?;
        let name = node.attribute("name").unwrap_or(port_name);
        let period_ns = node
            .attribute("period")
            .ok_or_else(|| anyhow!("clock '{name}' is missing its 'period' attribute"))?
            .parse::<f64>()
            .with_context(|| format!("clock '{name}' has an invalid period"))?;
        if !period_ns.is_finite() || period_ns <= 0.0 {
            bail!("clock '{name}' period must be a positive finite value");
        }
        if !names.insert(name.to_string()) {
            bail!("duplicate clock constraint name '{name}'");
        }
        if !ports.insert(port_name.to_string()) {
            bail!("multiple clock constraints target port '{port_name}'");
        }
        clocks.push(ClockConstraint {
            name: name.to_string(),
            port_name: port_name.to_string(),
            period_ns,
        });
    }
    Ok(clocks)
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
    apply_constraints_checked(design, arch, &constraints)?;
    Ok(constraints)
}

pub fn apply_constraints_checked(
    design: &mut Design,
    arch: &Arch,
    constraints: &[ConstraintEntry],
) -> Result<()> {
    for constraint in constraints {
        if !design
            .ports
            .iter()
            .any(|port| port.name == constraint.port_name)
        {
            bail!(
                "constraint references unknown design port '{}'",
                constraint.port_name
            );
        }
        if arch.pad(&constraint.pin_name).is_none() {
            bail!(
                "constraint for port '{}' references unknown package pin '{}'",
                constraint.port_name,
                constraint.pin_name
            );
        }
    }
    apply_constraints(design, arch, constraints);
    Ok(())
}

pub fn apply_constraints(design: &mut Design, arch: &Arch, constraints: &[ConstraintEntry]) {
    for constraint in constraints {
        if let Some(port) = design
            .ports
            .iter_mut()
            .find(|port| port.name == constraint.port_name)
        {
            port.pin = Some(constraint.pin_name.clone());
            if let Some(pad) = arch.pad(&constraint.pin_name) {
                assign_pad_site(port, pad);
            }
        }
    }
}

pub fn ensure_port_positions(design: &mut Design, arch: &Arch) {
    for (index, port) in design.ports.iter_mut().enumerate() {
        if let Some(pad) = port.pin.as_deref().and_then(|pin| arch.pad(pin)) {
            assign_pad_site(port, pad);
            continue;
        }
        if let Some((x, y)) = port.x.zip(port.y) {
            if let Some(pad) = arch.pad_at_site(x, y, port.z, None) {
                assign_pad_site(port, pad);
            } else {
                port.z.get_or_insert(0);
            }
            continue;
        }
        if let Some(pad) = arch.fallback_pad(index) {
            assign_pad_site(port, pad);
        } else {
            let (x, y) = arch.fallback_port_position(index, port.direction.is_input_like());
            port.x = Some(x);
            port.y = Some(y);
            port.z = Some(0);
        }
    }
}

fn assign_pad_site(port: &mut crate::ir::Port, pad: &crate::resource::Pad) {
    port.pin = Some(pad.name.clone());
    port.x = Some(pad.x);
    port.y = Some(pad.y);
    port.z = Some(pad.z);
}

pub fn ensure_cluster_positions(design: &Design) -> Result<()> {
    for cluster in &design.clusters {
        if cluster.x.is_none() || cluster.y.is_none() {
            bail!("cluster {} is missing placement coordinates", cluster.name);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ConstraintEntry, apply_constraints_checked, ensure_port_positions, load_constraint_set,
    };
    use crate::{
        ir::{Design, Port},
        resource::{Arch, Pad},
    };

    fn arch_with_pad() -> Arch {
        let pad = Pad {
            name: "P3".to_string(),
            x: 1,
            y: 2,
            z: 3,
            tile_name: "L1".to_string(),
            tile_type: "LEFT".to_string(),
            ..Pad::default()
        };
        let mut arch = Arch::default();
        arch.pad_lookup.insert(pad.name.clone(), (pad.x, pad.y));
        arch.pad_sites.insert(pad.name.clone(), pad.clone());
        arch.pads.push(pad);
        arch
    }

    #[test]
    fn checked_constraints_reject_unknown_pins() {
        let arch = arch_with_pad();
        let mut design = Design {
            ports: vec![Port::input("clk")],
            ..Design::default()
        };
        let constraints = [ConstraintEntry {
            port_name: "clk".to_string(),
            pin_name: "P1".to_string(),
        }];

        let error = apply_constraints_checked(&mut design, &arch, &constraints)
            .expect_err("unknown package pin must fail");

        assert!(error.to_string().contains("P1"));
        assert!(error.to_string().contains("clk"));
    }

    #[test]
    fn checked_constraints_assign_the_complete_pad_site() {
        let arch = arch_with_pad();
        let mut design = Design {
            ports: vec![Port::input("clk")],
            ..Design::default()
        };
        let constraints = [ConstraintEntry {
            port_name: "clk".to_string(),
            pin_name: "P3".to_string(),
        }];

        apply_constraints_checked(&mut design, &arch, &constraints).expect("valid constraint");

        let port = &design.ports[0];
        assert_eq!(port.pin.as_deref(), Some("P3"));
        assert_eq!((port.x, port.y, port.z), (Some(1), Some(2), Some(3)));
    }

    #[test]
    fn fallback_ports_bind_to_one_complete_pad_site() {
        let arch = arch_with_pad();
        let mut design = Design {
            ports: vec![Port::input("din")],
            ..Design::default()
        };

        ensure_port_positions(&mut design, &arch);

        let port = &design.ports[0];
        assert_eq!(port.pin.as_deref(), Some("P3"));
        assert_eq!((port.x, port.y, port.z), (Some(1), Some(2), Some(3)));
    }

    #[test]
    fn loads_pin_and_clock_constraints_together() {
        let file = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(
            file.path(),
            "<design><port name=\"clk\" position=\"P3\"/><clock name=\"sys\" port=\"clk\" period=\"10.5\"/></design>",
        )
        .expect("write constraints");

        let constraints = load_constraint_set(file.path()).expect("constraint set");

        assert_eq!(constraints.pins.len(), 1);
        assert_eq!(constraints.clocks.len(), 1);
        assert_eq!(constraints.clocks[0].name, "sys");
        assert_eq!(constraints.clocks[0].port_name, "clk");
        assert!((constraints.clocks[0].period_ns - 10.5).abs() < f64::EPSILON);
    }

    #[test]
    fn rejects_non_positive_clock_periods() {
        let file = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(
            file.path(),
            "<design><clock port=\"clk\" period=\"0\"/></design>",
        )
        .expect("write constraints");

        let error = load_constraint_set(file.path()).expect_err("zero period must fail");

        assert!(error.to_string().contains("positive finite"));
    }
}
