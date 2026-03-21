use crate::{
    domain::{ConstantKind, PrimitiveKind},
    edif::load_edif,
    io::load_design,
    ir::{Cell, Design},
    normalize::prune_disconnected_nets,
    report::{StageOutput, StageReport},
};
use anyhow::{Result, bail};
use std::{
    collections::BTreeSet,
    fmt::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct MapOptions {
    pub lut_size: usize,
    pub cell_library: Option<PathBuf>,
    pub emit_structural_verilog: bool,
}

impl Default for MapOptions {
    fn default() -> Self {
        Self {
            lut_size: 4,
            cell_library: None,
            emit_structural_verilog: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MapArtifact {
    pub design: Design,
    pub structural_verilog: Option<String>,
}

pub fn load_input(path: &Path) -> Result<Design> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "edf" | "edif" => load_edif(path),
        "xml" | "json" => load_design(path),
        "v" | "sv" => bail!(
            "Verilog is not a primary frontend in this rewrite. Use Yosys to generate EDIF first."
        ),
        _ => bail!("unsupported map input format for {}", path.display()),
    }
}

pub fn run(mut design: Design, options: &MapOptions) -> Result<StageOutput<MapArtifact>> {
    design.stage = "mapped".to_string();
    design.metadata.lut_size = options.lut_size;
    if design.metadata.source_format.is_empty() {
        design.metadata.source_format = "ir".to_string();
    }
    if let Some(celllib) = &options.cell_library {
        design.note(format!(
            "Mapping referenced cell library {}",
            celllib.display()
        ));
    }

    for cell in &mut design.cells {
        if cell.is_lut() && cell.property("lut_init").is_none() {
            let width = infer_lut_width(&cell.type_name).max(1);
            let lut_init = inherited_lut_init(cell).unwrap_or_else(|| default_lut_mask(width));
            cell.set_property("lut_init", lut_init);
        }
        if matches!(cell.primitive_kind(), PrimitiveKind::Generic) {
            let input_count = cell.inputs.len().clamp(1, options.lut_size.max(1));
            cell.kind = "lut".to_string();
            cell.type_name = format!("LUT{}", input_count.max(2));
            if cell.property("lut_init").is_none() {
                let lut_init =
                    inherited_lut_init(cell).unwrap_or_else(|| default_lut_mask(input_count));
                cell.set_property("lut_init", lut_init);
            }
        }
    }

    let lowered_constants = lower_constant_sources(&mut design, options.lut_size.max(1));
    if lowered_constants > 0 {
        design.note(format!(
            "Lowered {lowered_constants} constant source cells into LUT-backed drivers."
        ));
    }

    prune_disconnected_nets(&mut design);
    let structural_verilog = options
        .emit_structural_verilog
        .then(|| export_structural_verilog(&design));

    let mut report = StageReport::new("map");
    report.push(format!(
        "Mapped {} cells and {} nets.",
        design.cells.len(),
        design.nets.len()
    ));

    Ok(StageOutput {
        value: MapArtifact {
            design,
            structural_verilog,
        },
        report,
    })
}

pub fn export_structural_verilog(design: &Design) -> String {
    let mut output = String::new();
    let port_list = design
        .ports
        .iter()
        .map(|port| port.name.clone())
        .collect::<Vec<_>>();
    let _ = writeln!(output, "module {}({});", design.name, port_list.join(", "));
    for port in &design.ports {
        let _ = writeln!(output, "  {} {};", port.direction.as_str(), port.name);
    }
    for net in &design.nets {
        if !design.ports.iter().any(|port| port.name == net.name) {
            let _ = writeln!(output, "  wire {};", net.name);
        }
    }
    for cell in &design.cells {
        let mut pins = Vec::new();
        for input in &cell.inputs {
            pins.push(format!(".{}({})", input.port, input.net));
        }
        for output_pin in &cell.outputs {
            pins.push(format!(".{}({})", output_pin.port, output_pin.net));
        }
        let _ = writeln!(
            output,
            "  {} {} ({});",
            cell.type_name,
            cell.name,
            pins.join(", ")
        );
    }
    let _ = writeln!(output, "endmodule");
    output
}

fn infer_lut_width(type_name: &str) -> usize {
    type_name
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or(2)
}

fn default_lut_mask(width: usize) -> String {
    match width {
        0 | 1 => "2".to_string(),
        2 => "8".to_string(),
        3 => "80".to_string(),
        4 => "8000".to_string(),
        5 => "80000000".to_string(),
        _ => "AAAAAAAA".to_string(),
    }
}

fn inherited_lut_init(cell: &Cell) -> Option<String> {
    cell.property("init").map(ToOwned::to_owned)
}

fn lower_constant_sources(design: &mut Design, lut_size: usize) -> usize {
    let lut_size = lut_size.max(1);
    let mut lowered = BTreeSet::new();

    for cell in &mut design.cells {
        let Some(init) = constant_lut_init(cell, lut_size) else {
            continue;
        };
        if cell.outputs.is_empty() {
            continue;
        }
        cell.kind = "lut".to_string();
        cell.type_name = format!("LUT{lut_size}");
        cell.inputs.clear();
        for output in &mut cell.outputs {
            output.port = "O".to_string();
        }
        cell.set_property("lut_init", init);
        lowered.insert(cell.name.clone());
    }

    if lowered.is_empty() {
        return 0;
    }

    for net in &mut design.nets {
        if let Some(driver) = &mut net.driver
            && driver.is_cell()
            && lowered.contains(&driver.name)
        {
            driver.pin = "O".to_string();
        }
    }

    lowered.len()
}

fn constant_lut_init(cell: &Cell, lut_size: usize) -> Option<String> {
    match cell.constant_kind() {
        Some(ConstantKind::Zero) => Some("0".to_string()),
        Some(ConstantKind::One) => Some(all_ones_truth_table(lut_size)),
        Some(ConstantKind::Unknown) | None => None,
    }
}

fn all_ones_truth_table(lut_size: usize) -> String {
    let bits = 1usize.checked_shl(lut_size.min(7) as u32).unwrap_or(128);
    if bits >= 128 {
        return u128::MAX.to_string();
    }
    ((1u128 << bits) - 1).to_string()
}

#[cfg(test)]
mod tests {
    use super::{MapOptions, all_ones_truth_table, run};
    use crate::{
        ir::{Cell, CellPin, Design, Endpoint, Net, Property},
        map::MapArtifact,
    };
    use anyhow::Result;

    fn mapped_design() -> Design {
        Design {
            name: "const-lower".to_string(),
            cells: vec![
                Cell {
                    name: "GND".to_string(),
                    kind: "constant".to_string(),
                    type_name: "GND".to_string(),
                    outputs: vec![CellPin {
                        port: "G".to_string(),
                        net: "gnd_net".to_string(),
                    }],
                    ..Cell::default()
                },
                Cell {
                    name: "VCC".to_string(),
                    kind: "constant".to_string(),
                    type_name: "VCC".to_string(),
                    outputs: vec![CellPin {
                        port: "P".to_string(),
                        net: "vcc_net".to_string(),
                    }],
                    ..Cell::default()
                },
                Cell {
                    name: "sink".to_string(),
                    kind: "lut".to_string(),
                    type_name: "LUT4".to_string(),
                    inputs: vec![
                        CellPin {
                            port: "ADR0".to_string(),
                            net: "gnd_net".to_string(),
                        },
                        CellPin {
                            port: "ADR1".to_string(),
                            net: "vcc_net".to_string(),
                        },
                    ],
                    outputs: vec![CellPin {
                        port: "O".to_string(),
                        net: "out_net".to_string(),
                    }],
                    ..Cell::default()
                },
            ],
            nets: vec![
                Net {
                    name: "gnd_net".to_string(),
                    driver: Some(Endpoint {
                        kind: "cell".to_string(),
                        name: "GND".to_string(),
                        pin: "G".to_string(),
                    }),
                    sinks: vec![Endpoint {
                        kind: "cell".to_string(),
                        name: "sink".to_string(),
                        pin: "ADR0".to_string(),
                    }],
                    ..Net::default()
                },
                Net {
                    name: "vcc_net".to_string(),
                    driver: Some(Endpoint {
                        kind: "cell".to_string(),
                        name: "VCC".to_string(),
                        pin: "P".to_string(),
                    }),
                    sinks: vec![Endpoint {
                        kind: "cell".to_string(),
                        name: "sink".to_string(),
                        pin: "ADR1".to_string(),
                    }],
                    ..Net::default()
                },
            ],
            ..Design::default()
        }
    }

    fn mapped_artifact() -> Result<MapArtifact> {
        Ok(run(
            mapped_design(),
            &MapOptions {
                lut_size: 4,
                ..MapOptions::default()
            },
        )?
        .value)
    }

    #[test]
    fn map_lowers_constant_sources_into_lut_drivers() -> Result<()> {
        let artifact = mapped_artifact()?;
        let vcc_mask = all_ones_truth_table(4);
        let gnd = artifact
            .design
            .cells
            .iter()
            .find(|cell| cell.name == "GND")
            .expect("gnd cell");
        let vcc = artifact
            .design
            .cells
            .iter()
            .find(|cell| cell.name == "VCC")
            .expect("vcc cell");

        assert_eq!(gnd.kind, "lut");
        assert_eq!(gnd.type_name, "LUT4");
        assert_eq!(gnd.property("lut_init"), Some("0"));
        assert_eq!(gnd.outputs.first().map(|pin| pin.port.as_str()), Some("O"));

        assert_eq!(vcc.kind, "lut");
        assert_eq!(vcc.type_name, "LUT4");
        assert_eq!(vcc.property("lut_init"), Some(vcc_mask.as_str()));
        assert_eq!(vcc.outputs.first().map(|pin| pin.port.as_str()), Some("O"));

        Ok(())
    }

    #[test]
    fn map_updates_constant_net_driver_pins_after_lowering() -> Result<()> {
        let artifact = mapped_artifact()?;
        let gnd_net = artifact
            .design
            .nets
            .iter()
            .find(|net| net.name == "gnd_net")
            .expect("gnd net");
        let vcc_net = artifact
            .design
            .nets
            .iter()
            .find(|net| net.name == "vcc_net")
            .expect("vcc net");

        assert_eq!(
            gnd_net.driver.as_ref().map(|driver| driver.pin.as_str()),
            Some("O")
        );
        assert_eq!(
            vcc_net.driver.as_ref().map(|driver| driver.pin.as_str()),
            Some("O")
        );

        Ok(())
    }

    #[test]
    fn map_promotes_existing_init_property_into_lut_init() -> Result<()> {
        let design = Design {
            cells: vec![Cell {
                name: "pass".to_string(),
                kind: "lut".to_string(),
                type_name: "LUT2".to_string(),
                properties: vec![Property {
                    key: "init".to_string(),
                    value: "10".to_string(),
                }],
                ..Cell::default()
            }],
            ..Design::default()
        };

        let artifact = run(design, &MapOptions::default())?.value;
        let cell = artifact.design.cells.first().expect("lut cell");
        assert_eq!(cell.property("init"), Some("10"));
        assert_eq!(cell.property("lut_init"), Some("10"));

        Ok(())
    }
}
