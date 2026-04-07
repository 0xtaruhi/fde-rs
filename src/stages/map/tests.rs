use super::{MapArtifact, MapOptions, export_structural_verilog, run};
use crate::{
    io::{load_design, save_design},
    ir::{Cell, CellKind, Design, Endpoint, EndpointKind, Net, Port},
    map::{
        lut::all_ones_truth_table, lut::all_zeros_truth_table,
        rewrite::normalize_repeated_lut_inputs,
    },
};
use anyhow::Result;
use tempfile::TempDir;

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
    let gnd_mask = all_zeros_truth_table(4);
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
    assert_eq!(gnd.property("lut_init"), Some(gnd_mask.as_str()));
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
fn map_canonicalizes_edif_init_property_into_lut_init_hex() -> Result<()> {
    let mut design = Design {
        name: "top".to_string(),
        cells: vec![
            Cell::lut("u0", "LUT2")
                .with_input("ADR0", "a")
                .with_input("ADR1", "b")
                .with_output("O", "y"),
        ],
        ..Design::default()
    };
    design.metadata.source_format = "edif".to_string();
    design.cells[0].set_property("init", "10");

    let artifact = run(design, &MapOptions::default())?.value;
    let cell = artifact
        .design
        .cells
        .iter()
        .find(|cell| cell.name == "u0")
        .expect("u0");

    assert_eq!(cell.property("init"), Some("10"));
    assert_eq!(cell.property("lut_init"), Some("0xA"));

    Ok(())
}

#[test]
fn normalize_repeated_lut_inputs_rebuilds_inputs_and_pin_map() {
    let mut design = Design {
        name: "dup-lut".to_string(),
        cells: vec![
            Cell::lut("u0", "LUT4")
                .with_input("ADR0", "a")
                .with_input("ADR1", "a")
                .with_input("ADR2", "b")
                .with_input("ADR3", "b")
                .with_output("O", "y"),
        ],
        nets: vec![
            Net::new("a")
                .with_sink(Endpoint::new(EndpointKind::Cell, "u0", "ADR0"))
                .with_sink(Endpoint::new(EndpointKind::Cell, "u0", "ADR1")),
            Net::new("b")
                .with_sink(Endpoint::new(EndpointKind::Cell, "u0", "ADR2"))
                .with_sink(Endpoint::new(EndpointKind::Cell, "u0", "ADR3")),
        ],
        ..Design::default()
    };
    design.cells[0].set_property("lut_init", "0x8000");

    let normalized = normalize_repeated_lut_inputs(&mut design);
    let cell = &design.cells[0];

    assert_eq!(normalized, 1);
    assert_eq!(cell.type_name, "LUT2");
    assert_eq!(cell.inputs.len(), 2);
    assert_eq!(cell.inputs[0].port, "ADR0");
    assert_eq!(cell.inputs[0].net, "a");
    assert_eq!(cell.inputs[1].port, "ADR1");
    assert_eq!(cell.inputs[1].net, "b");
    assert_eq!(cell.property("lut_init"), Some("0x8"));
    assert_eq!(cell.property("pin_map_ADR0"), Some("0,1"));
    assert_eq!(cell.property("pin_map_ADR1"), Some("2,3"));

    let a_net = design
        .nets
        .iter()
        .find(|net| net.name == "a")
        .expect("a net");
    let b_net = design
        .nets
        .iter()
        .find(|net| net.name == "b")
        .expect("b net");
    assert_eq!(a_net.sinks.len(), 1);
    assert_eq!(a_net.sinks[0].pin, "ADR0");
    assert_eq!(b_net.sinks.len(), 1);
    assert_eq!(b_net.sinks[0].pin, "ADR1");
}

#[test]
fn map_buffers_ff_data_inputs_that_are_not_lut_driven() -> Result<()> {
    let design = Design {
        name: "ff-buf".to_string(),
        ports: vec![Port::input("din"), Port::input("clk"), Port::output("q")],
        cells: vec![
            Cell::ff("ff0", "DFFHQ")
                .with_input("D", "din")
                .with_input("CK", "clk")
                .with_output("Q", "q_net"),
        ],
        nets: vec![
            Net::new("din")
                .with_driver(Endpoint::new(EndpointKind::Port, "din", "din"))
                .with_sink(Endpoint::new(EndpointKind::Cell, "ff0", "D")),
            Net::new("clk")
                .with_driver(Endpoint::new(EndpointKind::Port, "clk", "clk"))
                .with_sink(Endpoint::new(EndpointKind::Cell, "ff0", "CK")),
            Net::new("q_net")
                .with_driver(Endpoint::new(EndpointKind::Cell, "ff0", "Q"))
                .with_sink(Endpoint::new(EndpointKind::Port, "q", "q")),
        ],
        ..Design::default()
    };

    let artifact = run(design, &MapOptions::default())?.value;
    let buffer = artifact
        .design
        .cells
        .iter()
        .find(|cell| cell.name == "ff0__d_buf_lut")
        .expect("buffer LUT");
    let ff = artifact
        .design
        .cells
        .iter()
        .find(|cell| cell.name == "ff0")
        .expect("ff0");
    let buffered_net = artifact
        .design
        .nets
        .iter()
        .find(|net| net.name == "ff0__d_buf_net")
        .expect("buffered net");
    let source_net = artifact
        .design
        .nets
        .iter()
        .find(|net| net.name == "din")
        .expect("source net");

    assert_eq!(buffer.type_name, "LUT1");
    assert_eq!(buffer.property("lut_init"), Some("0x2"));
    assert_eq!(
        ff.inputs
            .iter()
            .find(|pin| pin.port == "D")
            .map(|pin| pin.net.as_str()),
        Some("ff0__d_buf_net")
    );
    assert_eq!(
        buffered_net
            .driver
            .as_ref()
            .map(|driver| (driver.name.as_str(), driver.pin.as_str())),
        Some(("ff0__d_buf_lut", "O"))
    );
    assert!(
        !source_net
            .sinks
            .iter()
            .any(|sink| sink.name == "ff0" && sink.pin == "D")
    );

    Ok(())
}

#[test]
fn map_normalizes_ramb4_single_port_cells_into_canonical_blockram_1() -> Result<()> {
    let design = Design {
        name: "bram_single".to_string(),
        cells: vec![
            Cell::new("ram0", CellKind::BlockRam, "RAMB4_S2")
                .with_input("CLK", "clk")
                .with_input("WE", "we")
                .with_input("RST", "rst")
                .with_input("EN", "en")
                .with_input("DI[0]", "di0")
                .with_input("DI[1]", "di1")
                .with_input("ADDR[0]", "a0")
                .with_input("ADDR[10]", "a10")
                .with_output("DO[0]", "do0")
                .with_output("DO[1]", "do1"),
        ],
        nets: vec![
            Net::new("clk")
                .with_driver(Endpoint::port("clk", "clk"))
                .with_sink(Endpoint::cell("ram0", "CLK")),
            Net::new("we")
                .with_driver(Endpoint::port("we", "we"))
                .with_sink(Endpoint::cell("ram0", "WE")),
            Net::new("rst")
                .with_driver(Endpoint::port("rst", "rst"))
                .with_sink(Endpoint::cell("ram0", "RST")),
            Net::new("en")
                .with_driver(Endpoint::port("en", "en"))
                .with_sink(Endpoint::cell("ram0", "EN")),
            Net::new("di0")
                .with_driver(Endpoint::port("di0", "di0"))
                .with_sink(Endpoint::cell("ram0", "DI[0]")),
            Net::new("di1")
                .with_driver(Endpoint::port("di1", "di1"))
                .with_sink(Endpoint::cell("ram0", "DI[1]")),
            Net::new("a0")
                .with_driver(Endpoint::port("a0", "a0"))
                .with_sink(Endpoint::cell("ram0", "ADDR[0]")),
            Net::new("a10")
                .with_driver(Endpoint::port("a10", "a10"))
                .with_sink(Endpoint::cell("ram0", "ADDR[10]")),
            Net::new("do0")
                .with_driver(Endpoint::cell("ram0", "DO[0]"))
                .with_sink(Endpoint::port("do0", "do0")),
            Net::new("do1")
                .with_driver(Endpoint::cell("ram0", "DO[1]"))
                .with_sink(Endpoint::port("do1", "do1")),
        ],
        ..Design::default()
    };

    let artifact = run(design, &MapOptions::default())?.value;
    let ram = artifact
        .design
        .cells
        .iter()
        .find(|cell| cell.name == "ram0")
        .expect("ram0");

    assert_eq!(ram.type_name, "BLOCKRAM_1");
    assert_eq!(ram.property("PORT_ATTR"), Some("2048X2"));
    assert_eq!(
        ram.inputs
            .iter()
            .find(|pin| pin.net == "di0")
            .map(|pin| pin.port.as_str()),
        Some("DI0")
    );
    assert_eq!(
        ram.inputs
            .iter()
            .find(|pin| pin.net == "di1")
            .map(|pin| pin.port.as_str()),
        Some("DI1")
    );
    assert_eq!(
        ram.inputs
            .iter()
            .find(|pin| pin.net == "a0")
            .map(|pin| pin.port.as_str()),
        Some("ADDR1")
    );
    assert_eq!(
        ram.inputs
            .iter()
            .find(|pin| pin.net == "a10")
            .map(|pin| pin.port.as_str()),
        Some("ADDR11")
    );
    assert_eq!(
        ram.outputs
            .iter()
            .find(|pin| pin.net == "do0")
            .map(|pin| pin.port.as_str()),
        Some("DO0")
    );
    assert_eq!(
        ram.outputs
            .iter()
            .find(|pin| pin.net == "do1")
            .map(|pin| pin.port.as_str()),
        Some("DO1")
    );

    let do0 = artifact
        .design
        .nets
        .iter()
        .find(|net| net.name == "do0")
        .expect("do0");
    let a0 = artifact
        .design
        .nets
        .iter()
        .find(|net| net.name == "a0")
        .expect("a0");
    assert_eq!(
        do0.driver.as_ref().map(|endpoint| endpoint.pin.as_str()),
        Some("DO0")
    );
    assert_eq!(
        a0.sinks.first().map(|endpoint| endpoint.pin.as_str()),
        Some("ADDR1")
    );

    Ok(())
}

#[test]
fn map_normalizes_ramb4_dual_port_cells_into_canonical_blockram_2() -> Result<()> {
    let mut ram = Cell::new("ram0", CellKind::BlockRam, "RAMB4_S1_S16")
        .with_input("CLKA", "clka")
        .with_input("WEA", "wea")
        .with_input("RSTA", "rsta")
        .with_input("ENA", "ena")
        .with_input("CLKB", "clkb")
        .with_input("WEB", "web")
        .with_input("RSTB", "rstb")
        .with_input("ENB", "enb")
        .with_input("DIA[0]", "dia0")
        .with_input("ADDRA[0]", "addra0")
        .with_input("ADDRA[11]", "addra11")
        .with_output("DOA[0]", "doa0")
        .with_input("DIB[15]", "dib15")
        .with_input("ADDRB[0]", "addrb0")
        .with_input("ADDRB[7]", "addrb7")
        .with_output("DOB[15]", "dob15");
    ram.set_property("INIT_00", "0123456789ABCDEF");

    let design = Design {
        name: "bram_dual".to_string(),
        cells: vec![ram],
        nets: vec![
            Net::new("clka")
                .with_driver(Endpoint::port("clka", "clka"))
                .with_sink(Endpoint::cell("ram0", "CLKA")),
            Net::new("wea")
                .with_driver(Endpoint::port("wea", "wea"))
                .with_sink(Endpoint::cell("ram0", "WEA")),
            Net::new("rsta")
                .with_driver(Endpoint::port("rsta", "rsta"))
                .with_sink(Endpoint::cell("ram0", "RSTA")),
            Net::new("ena")
                .with_driver(Endpoint::port("ena", "ena"))
                .with_sink(Endpoint::cell("ram0", "ENA")),
            Net::new("clkb")
                .with_driver(Endpoint::port("clkb", "clkb"))
                .with_sink(Endpoint::cell("ram0", "CLKB")),
            Net::new("web")
                .with_driver(Endpoint::port("web", "web"))
                .with_sink(Endpoint::cell("ram0", "WEB")),
            Net::new("rstb")
                .with_driver(Endpoint::port("rstb", "rstb"))
                .with_sink(Endpoint::cell("ram0", "RSTB")),
            Net::new("enb")
                .with_driver(Endpoint::port("enb", "enb"))
                .with_sink(Endpoint::cell("ram0", "ENB")),
            Net::new("dia0")
                .with_driver(Endpoint::port("dia0", "dia0"))
                .with_sink(Endpoint::cell("ram0", "DIA[0]")),
            Net::new("addra0")
                .with_driver(Endpoint::port("addra0", "addra0"))
                .with_sink(Endpoint::cell("ram0", "ADDRA[0]")),
            Net::new("addra11")
                .with_driver(Endpoint::port("addra11", "addra11"))
                .with_sink(Endpoint::cell("ram0", "ADDRA[11]")),
            Net::new("doa0")
                .with_driver(Endpoint::cell("ram0", "DOA[0]"))
                .with_sink(Endpoint::port("doa0", "doa0")),
            Net::new("dib15")
                .with_driver(Endpoint::port("dib15", "dib15"))
                .with_sink(Endpoint::cell("ram0", "DIB[15]")),
            Net::new("addrb0")
                .with_driver(Endpoint::port("addrb0", "addrb0"))
                .with_sink(Endpoint::cell("ram0", "ADDRB[0]")),
            Net::new("addrb7")
                .with_driver(Endpoint::port("addrb7", "addrb7"))
                .with_sink(Endpoint::cell("ram0", "ADDRB[7]")),
            Net::new("dob15")
                .with_driver(Endpoint::cell("ram0", "DOB[15]"))
                .with_sink(Endpoint::port("dob15", "dob15")),
        ],
        ..Design::default()
    };

    let artifact = run(design, &MapOptions::default())?.value;
    let ram = artifact
        .design
        .cells
        .iter()
        .find(|cell| cell.name == "ram0")
        .expect("ram0");

    assert_eq!(ram.type_name, "BLOCKRAM_2");
    assert_eq!(ram.property("PORTA_ATTR"), Some("4096X1"));
    assert_eq!(ram.property("PORTB_ATTR"), Some("256X16"));
    assert_eq!(ram.property("INIT_00"), Some("0123456789ABCDEF"));
    assert_eq!(
        ram.inputs
            .iter()
            .find(|pin| pin.net == "dia0")
            .map(|pin| pin.port.as_str()),
        Some("DIA0")
    );
    assert_eq!(
        ram.inputs
            .iter()
            .find(|pin| pin.net == "addra0")
            .map(|pin| pin.port.as_str()),
        Some("ADDRA0")
    );
    assert_eq!(
        ram.inputs
            .iter()
            .find(|pin| pin.net == "addra11")
            .map(|pin| pin.port.as_str()),
        Some("ADDRA11")
    );
    assert_eq!(
        ram.inputs
            .iter()
            .find(|pin| pin.net == "dib15")
            .map(|pin| pin.port.as_str()),
        Some("DIB15")
    );
    assert_eq!(
        ram.inputs
            .iter()
            .find(|pin| pin.net == "addrb0")
            .map(|pin| pin.port.as_str()),
        Some("ADDRB4")
    );
    assert_eq!(
        ram.inputs
            .iter()
            .find(|pin| pin.net == "addrb7")
            .map(|pin| pin.port.as_str()),
        Some("ADDRB11")
    );
    assert_eq!(
        ram.outputs
            .iter()
            .find(|pin| pin.net == "doa0")
            .map(|pin| pin.port.as_str()),
        Some("DOA0")
    );
    assert_eq!(
        ram.outputs
            .iter()
            .find(|pin| pin.net == "dob15")
            .map(|pin| pin.port.as_str()),
        Some("DOB15")
    );

    let addrb0 = artifact
        .design
        .nets
        .iter()
        .find(|net| net.name == "addrb0")
        .expect("addrb0");
    let dob15 = artifact
        .design
        .nets
        .iter()
        .find(|net| net.name == "dob15")
        .expect("dob15");
    assert_eq!(
        addrb0.sinks.first().map(|endpoint| endpoint.pin.as_str()),
        Some("ADDRB4")
    );
    assert_eq!(
        dob15.driver.as_ref().map(|endpoint| endpoint.pin.as_str()),
        Some("DOB15")
    );

    Ok(())
}

#[test]
fn mapped_xml_roundtrip_preserves_inserted_lut1_helpers() -> Result<()> {
    let design = Design {
        name: "ff-buf-roundtrip".to_string(),
        ports: vec![Port::input("din"), Port::input("clk"), Port::output("q")],
        cells: vec![
            Cell::ff("ff0", "DFFHQ")
                .with_input("D", "din")
                .with_input("CK", "clk")
                .with_output("Q", "q_net"),
        ],
        nets: vec![
            Net::new("din")
                .with_driver(Endpoint::new(EndpointKind::Port, "din", "din"))
                .with_sink(Endpoint::new(EndpointKind::Cell, "ff0", "D")),
            Net::new("clk")
                .with_driver(Endpoint::new(EndpointKind::Port, "clk", "clk"))
                .with_sink(Endpoint::new(EndpointKind::Cell, "ff0", "CK")),
            Net::new("q_net")
                .with_driver(Endpoint::new(EndpointKind::Cell, "ff0", "Q"))
                .with_sink(Endpoint::new(EndpointKind::Port, "q", "q")),
        ],
        ..Design::default()
    };

    let artifact = run(design, &MapOptions::default())?.value;
    let temp = TempDir::new()?;
    let path = temp.path().join("mapped.xml");
    save_design(&artifact.design, &path)?;
    let roundtripped = load_design(&path)?;

    let buffer = roundtripped
        .cells
        .iter()
        .find(|cell| cell.type_name == "LUT1")
        .expect("buffer LUT after mapped XML roundtrip");
    assert_eq!(buffer.type_name, "LUT1");
    assert_eq!(buffer.property("lut_init"), Some("0x2"));

    Ok(())
}

#[test]
fn map_rejects_unsupported_generic_cells_instead_of_silently_rewriting_them() {
    let design = Design {
        name: "generic-macc".to_string(),
        cells: vec![
            Cell::new("mac0", CellKind::Generic, "$macc_v2")
                .with_input("A[0]", "a0")
                .with_input("B[0]", "b0")
                .with_output("Y[0]", "y0")
                .with_output("Y[1]", "y1"),
        ],
        ..Design::default()
    };

    let err = run(design, &MapOptions::default()).expect_err("generic arithmetic cell must fail");
    let message = err.to_string();
    assert!(message.contains("$macc_v2"));
    assert!(message.contains("maccmap -unmap"));
}
