use crate::{ir::Design, resource::Arch};
use anyhow::{Context, Result, anyhow, bail};
use roxmltree::Document;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path, sync::Arc};

pub type SharedConstraints = Arc<[ConstraintEntry]>;
pub type SharedClockConstraints = Arc<[ClockConstraint]>;
pub type SharedIoDelayConstraints = Arc<[IoDelayConstraint]>;
pub type SharedClockUncertainties = Arc<[ClockUncertaintyConstraint]>;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IoDelayConstraint {
    pub port_name: String,
    pub clock_name: String,
    pub delay_ns: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClockUncertaintyConstraint {
    pub clock_name: String,
    pub setup_ns: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SdcConstraintSet {
    pub clocks: Vec<ClockConstraint>,
    pub input_delays: Vec<IoDelayConstraint>,
    pub output_delays: Vec<IoDelayConstraint>,
    pub clock_uncertainties: Vec<ClockUncertaintyConstraint>,
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

/// Loads the strict SDC subset accepted by FDE.
///
/// Supported commands are `create_clock`, `set_input_delay`,
/// `set_output_delay`, and setup `set_clock_uncertainty`.
/// Unsupported commands are rejected instead of being silently ignored.
pub fn load_sdc_constraints(path: &Path) -> Result<SdcConstraintSet> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read SDC file {}", path.display()))?;
    let mut constraints = SdcConstraintSet::default();
    for (line_number, line) in logical_sdc_lines(&text) {
        let command = SdcCommand::new(path, line_number, &line);
        match command.name {
            "create_clock" => constraints.clocks.push(command.create_clock()?),
            "set_input_delay" => constraints.input_delays.push(command.io_delay()?),
            "set_output_delay" => constraints.output_delays.push(command.io_delay()?),
            "set_clock_uncertainty" => constraints
                .clock_uncertainties
                .push(command.clock_uncertainty()?),
            _ => bail!(
                "unsupported SDC command at {}:{line_number}: {line}",
                path.display()
            ),
        }
    }
    validate_clock_constraints(&constraints.clocks)?;
    validate_sdc_constraints(&constraints)?;
    Ok(constraints)
}

pub fn load_sdc_clocks(path: &Path) -> Result<Vec<ClockConstraint>> {
    Ok(load_sdc_constraints(path)?.clocks)
}

pub fn merge_clock_constraints(
    existing: &mut Vec<ClockConstraint>,
    additional: Vec<ClockConstraint>,
) -> Result<()> {
    existing.extend(additional);
    validate_clock_constraints(existing)
}

struct SdcCommand<'a> {
    path: &'a Path,
    line_number: usize,
    name: &'a str,
    tokens: Vec<&'a str>,
}

impl<'a> SdcCommand<'a> {
    fn new(path: &'a Path, line_number: usize, line: &'a str) -> Self {
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        Self {
            path,
            line_number,
            name: tokens.first().copied().unwrap_or_default(),
            tokens,
        }
    }

    fn create_clock(&self) -> Result<ClockConstraint> {
        self.reject_options(&["-name", "-period"])?;
        let period = self
            .option("-period")
            .ok_or_else(|| self.missing("-period"))?;
        let period_ns = period
            .parse()
            .with_context(|| format!("{} has invalid period '{period}'", self.location()))?;
        self.validate_time(period_ns, false)?;
        let port_name = self.target("get_ports").ok_or_else(|| {
            anyhow!(
                "{} must target exactly one [get_ports <port>]",
                self.location()
            )
        })?;
        if port_name.is_empty() {
            bail!("{} has an empty port name", self.location());
        }
        Ok(ClockConstraint {
            name: self
                .option("-name")
                .map_or_else(|| port_name.clone(), clean_sdc_word),
            port_name,
            period_ns,
        })
    }

    fn io_delay(&self) -> Result<IoDelayConstraint> {
        self.reject_options(&["-clock"])?;
        let clock_name = self
            .option("-clock")
            .ok_or_else(|| self.missing("-clock"))?;
        let port_name = self
            .target("get_ports")
            .ok_or_else(|| anyhow!("{} must target [get_ports <port>]", self.location()))?;
        let delay_ns = self
            .number(&["-clock"])
            .ok_or_else(|| self.missing("its delay value"))?;
        self.validate_time(delay_ns, true)?;
        Ok(IoDelayConstraint {
            port_name,
            clock_name: clean_sdc_word(clock_name),
            delay_ns,
        })
    }

    fn clock_uncertainty(&self) -> Result<ClockUncertaintyConstraint> {
        self.reject_options(&["-setup", "-hold"])?;
        if self.tokens.contains(&"-hold") {
            bail!(
                "{} -hold is unsupported because hold analysis is not implemented",
                self.location()
            );
        }
        let clock_name = self
            .target("get_clocks")
            .ok_or_else(|| anyhow!("{} must target one [get_clocks <clock>]", self.location()))?;
        let setup_ns = self
            .number(&[])
            .ok_or_else(|| self.missing("its uncertainty value"))?;
        self.validate_time(setup_ns, true)?;
        Ok(ClockUncertaintyConstraint {
            clock_name,
            setup_ns,
        })
    }

    fn option(&self, name: &str) -> Option<&'a str> {
        self.tokens
            .iter()
            .position(|token| *token == name)
            .and_then(|index| self.tokens.get(index + 1).copied())
    }

    fn target(&self, collection: &str) -> Option<String> {
        let index = self
            .tokens
            .iter()
            .position(|token| token.trim_start_matches('[') == collection)?;
        let value = self.tokens.get(index + 1)?;
        value.ends_with(']').then(|| clean_sdc_word(value))
    }

    fn number(&self, options_with_values: &[&str]) -> Option<f64> {
        let mut tokens = self.tokens.iter().skip(1);
        while let Some(token) = tokens.next() {
            if options_with_values.contains(token) {
                tokens.next();
            } else if !token.starts_with('-')
                && !token.contains("get_ports")
                && !token.contains("get_clocks")
                && let Ok(value) = clean_sdc_word(token).parse()
            {
                return Some(value);
            }
        }
        None
    }

    fn reject_options(&self, allowed: &[&str]) -> Result<()> {
        if let Some(option) = self
            .tokens
            .iter()
            .skip(1)
            .find(|token| token.starts_with('-') && !allowed.contains(token))
        {
            bail!(
                "unsupported {} option '{option}' at {}:{}",
                self.name,
                self.path.display(),
                self.line_number
            );
        }
        Ok(())
    }

    fn validate_time(&self, value: f64, allow_zero: bool) -> Result<()> {
        if !value.is_finite() || value < 0.0 || (!allow_zero && value == 0.0) {
            let requirement = if allow_zero {
                "a non-negative finite value"
            } else {
                "a positive finite value"
            };
            bail!("{} requires {requirement}", self.location());
        }
        Ok(())
    }

    fn location(&self) -> String {
        format!(
            "{} at {}:{}",
            self.name,
            self.path.display(),
            self.line_number
        )
    }

    fn missing(&self, value: &str) -> anyhow::Error {
        anyhow!("{} is missing {value}", self.location())
    }
}

fn logical_sdc_lines(text: &str) -> Vec<(usize, String)> {
    let mut lines = Vec::new();
    let mut pending = String::new();
    let mut start = 1;
    for (index, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        if pending.is_empty() {
            start = index + 1;
        } else {
            pending.push(' ');
        }
        let continued = line.ends_with('\\');
        pending.push_str(line.strip_suffix('\\').unwrap_or(line).trim());
        if !continued {
            lines.push((start, std::mem::take(&mut pending)));
        }
    }
    if !pending.is_empty() {
        lines.push((start, pending));
    }
    lines
}

fn clean_sdc_word(value: &str) -> String {
    value
        .trim_matches(|ch| matches!(ch, '[' | ']' | '{' | '}'))
        .to_string()
}

fn validate_sdc_constraints(constraints: &SdcConstraintSet) -> Result<()> {
    let clock_names = constraints
        .clocks
        .iter()
        .map(|clock| clock.name.as_str())
        .collect::<BTreeSet<_>>();
    for delay in constraints
        .input_delays
        .iter()
        .chain(&constraints.output_delays)
    {
        if !clock_names.contains(delay.clock_name.as_str()) {
            bail!(
                "I/O delay for port '{}' references unknown clock '{}'",
                delay.port_name,
                delay.clock_name
            );
        }
    }
    let mut uncertainty_clocks = BTreeSet::new();
    for uncertainty in &constraints.clock_uncertainties {
        if !clock_names.contains(uncertainty.clock_name.as_str()) {
            bail!(
                "clock uncertainty references unknown clock '{}'",
                uncertainty.clock_name
            );
        }
        if !uncertainty_clocks.insert(uncertainty.clock_name.as_str()) {
            bail!(
                "duplicate setup uncertainty for clock '{}'",
                uncertainty.clock_name
            );
        }
    }
    Ok(())
}

fn validate_clock_constraints(clocks: &[ClockConstraint]) -> Result<()> {
    let mut names = BTreeSet::new();
    let mut ports = BTreeSet::new();
    for clock in clocks {
        if !clock.period_ns.is_finite() || clock.period_ns <= 0.0 {
            bail!(
                "clock '{}' period must be a positive finite value",
                clock.name
            );
        }
        if !names.insert(clock.name.clone()) {
            bail!("duplicate clock constraint name '{}'", clock.name);
        }
        if !ports.insert(clock.port_name.clone()) {
            bail!(
                "multiple clock constraints target port '{}'",
                clock.port_name
            );
        }
    }
    Ok(())
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
    validate_clock_constraints(&clocks)?;
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
        load_sdc_clocks, load_sdc_constraints,
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

    #[test]
    fn loads_strict_sdc_create_clock_commands() {
        let file = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(
            file.path(),
            "# clocks\ncreate_clock -name sys -period 10.0 [get_ports clk]\n\
             create_clock -period 25.0 [get_ports {aux_clk}]\n",
        )
        .expect("write sdc");

        let clocks = load_sdc_clocks(file.path()).expect("load SDC");

        assert_eq!(clocks.len(), 2);
        assert_eq!(clocks[0].name, "sys");
        assert_eq!(clocks[0].port_name, "clk");
        assert_eq!(clocks[1].name, "aux_clk");
        assert_eq!(clocks[1].port_name, "aux_clk");
    }

    #[test]
    fn rejects_unsupported_sdc_commands() {
        let file = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(file.path(), "set_false_path -from a -to b\n").expect("write sdc");

        let error = load_sdc_clocks(file.path()).expect_err("unsupported command must fail");

        assert!(error.to_string().contains("unsupported SDC command"));
    }

    #[test]
    fn rejects_unsupported_sdc_options_instead_of_ignoring_them() {
        let file = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(
            file.path(),
            "create_clock -period 10 [get_ports clk]\n\
             set_input_delay -min -clock clk 1 [get_ports din]\n",
        )
        .expect("write sdc");

        let error = load_sdc_constraints(file.path()).expect_err("-min must not be ignored");

        assert!(
            error
                .to_string()
                .contains("unsupported set_input_delay option '-min'")
        );
    }

    #[test]
    fn loads_sdc_io_delays_and_setup_uncertainty() {
        let file = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(
            file.path(),
            "create_clock -name sys -period 10 [get_ports clk]\n\
             set_input_delay -clock sys 1.25 [get_ports din]\n\
             set_output_delay 2.5 -clock sys [get_ports dout]\n\
             set_clock_uncertainty -setup 0.2 [get_clocks sys]\n",
        )
        .expect("write sdc");

        let constraints = load_sdc_constraints(file.path()).expect("load SDC");

        assert_eq!(constraints.clocks.len(), 1);
        assert_eq!(constraints.input_delays[0].port_name, "din");
        assert!((constraints.input_delays[0].delay_ns - 1.25).abs() < f64::EPSILON);
        assert_eq!(constraints.output_delays[0].port_name, "dout");
        assert!((constraints.output_delays[0].delay_ns - 2.5).abs() < f64::EPSILON);
        assert_eq!(constraints.clock_uncertainties[0].clock_name, "sys");
        assert!((constraints.clock_uncertainties[0].setup_ns - 0.2).abs() < f64::EPSILON);
    }
}
