use crate::{
    domain::{CellKind, ConstantKind, PrimitiveKind},
    edif::load_edif,
    io::load_design,
    ir::{Cell, CellId, Design},
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
        if cell.is_lut() {
            canonicalize_lut_init(cell);
        }
        if cell.is_lut() && cell.property("lut_init").is_none() {
            let width = infer_lut_width(&cell.type_name).max(1);
            cell.set_property("lut_init", default_lut_mask(width));
        }
        if matches!(cell.primitive_kind(), PrimitiveKind::Generic) {
            let input_count = cell.inputs.len().clamp(1, options.lut_size.max(1));
            cell.kind = CellKind::Lut;
            cell.type_name = format!("LUT{}", input_count.max(2));
            canonicalize_lut_init(cell);
            if cell.property("lut_init").is_none() {
                cell.set_property("lut_init", default_lut_mask(input_count));
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
    let index = design.index();
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
        if index.port_id(&net.name).is_none() {
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

fn canonicalize_lut_init(cell: &mut Cell) {
    if let Some(value) = cell
        .property("lut_init")
        .or_else(|| cell.property("init"))
        .map(str::to_owned)
    {
        cell.set_property("lut_init", value);
    }
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

fn lower_constant_sources(design: &mut Design, lut_size: usize) -> usize {
    let lut_size = lut_size.max(1);
    let mut lowered = BTreeSet::new();

    for (cell_index, cell) in design.cells.iter_mut().enumerate() {
        let Some(init) = constant_lut_init(cell, lut_size) else {
            continue;
        };
        if cell.outputs.is_empty() {
            continue;
        }
        cell.kind = CellKind::Lut;
        cell.type_name = format!("LUT{lut_size}");
        cell.inputs.clear();
        for output in &mut cell.outputs {
            output.port = "O".to_string();
        }
        cell.set_property("lut_init", init);
        lowered.insert(CellId::new(cell_index));
    }

    if lowered.is_empty() {
        return 0;
    }

    let lowered_net_drivers = {
        let index = design.index();
        design
            .nets
            .iter()
            .map(|net| {
                net.driver
                    .as_ref()
                    .and_then(|driver| index.cell_for_endpoint(driver))
                    .is_some_and(|cell_id| lowered.contains(&cell_id))
            })
            .collect::<Vec<_>>()
    };

    for (net, is_lowered_driver) in design.nets.iter_mut().zip(lowered_net_drivers) {
        if is_lowered_driver && let Some(driver) = &mut net.driver {
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
    use super::{MapOptions, all_ones_truth_table, export_structural_verilog, run};
    use crate::{
        ir::{Cell, CellKind, Design, Endpoint, EndpointKind, Net, Port},
        map::MapArtifact,
    };
    use anyhow::Result;

    fn mapped_design() -> Design {
        Design {
            name: "const-lower".to_string(),
            cells: vec![
                Cell::new("GND", CellKind::Constant, "GND").with_output("G", "gnd_net"),
                Cell::new("VCC", CellKind::Constant, "VCC").with_output("P", "vcc_net"),
                Cell::new("sink", CellKind::Lut, "LUT4")
                    .with_input("ADR0", "gnd_net")
                    .with_input("ADR1", "vcc_net")
                    .with_output("O", "out_net"),
            ],
            nets: vec![
                Net::new("gnd_net")
                    .with_driver(Endpoint::new(EndpointKind::Cell, "GND", "G"))
                    .with_sink(Endpoint::new(EndpointKind::Cell, "sink", "ADR0")),
                Net::new("vcc_net")
                    .with_driver(Endpoint::new(EndpointKind::Cell, "VCC", "P"))
                    .with_sink(Endpoint::new(EndpointKind::Cell, "sink", "ADR1")),
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

        assert_eq!(gnd.kind, CellKind::Lut);
        assert_eq!(gnd.type_name, "LUT4");
        assert_eq!(gnd.property("lut_init"), Some("0"));
        assert_eq!(gnd.outputs.first().map(|pin| pin.port.as_str()), Some("O"));

        assert_eq!(vcc.kind, CellKind::Lut);
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
    fn structural_verilog_skips_port_named_nets_when_declaring_wires() {
        let design = Design {
            name: "top".to_string(),
            ports: vec![Port::input("in"), Port::output("out")],
            cells: vec![
                Cell::lut("u0", "LUT4")
                    .with_input("A", "in")
                    .with_output("O", "out")
                    .with_output("Q", "n1"),
            ],
            nets: vec![Net::new("out"), Net::new("n1")],
            ..Design::default()
        };

        let verilog = export_structural_verilog(&design);

        assert!(verilog.contains("wire n1;"));
        assert!(!verilog.contains("wire out;"));
    }

    #[test]
    fn map_canonicalizes_init_property_for_structural_luts() -> Result<()> {
        let design = Design {
            name: "top".to_string(),
            cells: vec![
                Cell::lut("u0", "LUT2")
                    .with_input("ADR0", "a")
                    .with_input("ADR1", "b")
                    .with_output("O", "y"),
            ],
            ..Design::default()
        };
        let mut design = design;
        design.cells[0].set_property("init", "10");

        let artifact = run(design, &MapOptions::default())?.value;
        let cell = artifact
            .design
            .cells
            .iter()
            .find(|cell| cell.name == "u0")
            .expect("u0");

        assert_eq!(cell.property("init"), Some("10"));
        assert_eq!(cell.property("lut_init"), Some("10"));

        Ok(())
    }
}
