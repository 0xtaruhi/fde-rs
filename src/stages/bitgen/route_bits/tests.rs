use crate::device::{DeviceCell, DeviceEndpoint};
use crate::domain::{EndpointKind, SiteKind};
use std::path::PathBuf;

use crate::{cil::load_cil, resource::load_arch};

use super::{
    graph::load_site_route_graphs,
    mapping::{endpoint_sink_nets, endpoint_source_nets},
    stitch::clock_spine_neighbors,
    types::{RouteNode, WireInterner},
    wire::parse_indexed_wire,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn wire_index_parser_normalizes_edge_and_long_wires() {
    assert_eq!(parse_indexed_wire("E9"), Some(("E", 9)));
    assert_eq!(parse_indexed_wire("LEFT_E10"), Some(("E", 10)));
    assert_eq!(parse_indexed_wire("H6W7"), Some(("H6W", 7)));
    assert_eq!(parse_indexed_wire("BOT_V6N2"), Some(("V6N", 2)));
    assert_eq!(parse_indexed_wire("S0_F_B4"), None);
}

#[test]
fn slice_mapping_interns_expected_source_and_sink_wires() {
    let mut wires = WireInterner::default();
    let lut = DeviceCell::new("u0", "LUT4").placed(
        SiteKind::LogicSlice,
        "S1",
        "BEL1",
        "T0",
        "LOGIC",
        (3, 4, 0),
    );
    let ff = DeviceCell::new("u1", "DFF").placed(
        SiteKind::LogicSlice,
        "S1",
        "BEL1",
        "T0",
        "LOGIC",
        (3, 4, 0),
    );
    let q = DeviceEndpoint::new(EndpointKind::Cell, "u0", "O", (3, 4, 0));
    let d = DeviceEndpoint::new(EndpointKind::Cell, "u0", "A", (3, 4, 0));
    let ck = DeviceEndpoint::new(EndpointKind::Cell, "u1", "CK", (3, 4, 0));

    let source = endpoint_source_nets(&lut, &q, &mut wires);
    let data = endpoint_sink_nets(&lut, &d, &mut wires);
    let clock = endpoint_sink_nets(&ff, &ck, &mut wires);

    assert_eq!(source.len(), 1);
    assert_eq!(data.len(), 1);
    assert_eq!(clock.len(), 1);
    assert_eq!(wires.resolve(source[0]), "S1_Y");
    assert_eq!(wires.resolve(data[0]), "S1_G_B1");
    assert_eq!(wires.resolve(clock[0]), "S1_CLK_B");
}

#[test]
fn io_and_clock_mapping_render_expected_wire_names() {
    let mut wires = WireInterner::default();
    let iob =
        DeviceCell::new("io0", "IOB").placed(SiteKind::Iob, "IOB7", "IO", "T1", "LEFT", (1, 2, 0));
    let gclk = DeviceCell::new("g0", "GCLK").placed(
        SiteKind::Gclk,
        "GCLKBUF0",
        "BUF",
        "T2",
        "CLKC",
        (5, 6, 2),
    );
    let input_pin = DeviceEndpoint::new(EndpointKind::Cell, "io0", "IN", (1, 2, 0));
    let output_pin = DeviceEndpoint::new(EndpointKind::Cell, "io0", "OUT", (1, 2, 0));
    let gclk_in = DeviceEndpoint::new(EndpointKind::Cell, "g0", "IN", (5, 6, 2));
    let gclk_out = DeviceEndpoint::new(EndpointKind::Cell, "g0", "OUT", (5, 6, 2));

    let iob_source = endpoint_source_nets(&iob, &input_pin, &mut wires);
    let iob_sink = endpoint_sink_nets(&iob, &output_pin, &mut wires);
    let gclk_sink = endpoint_sink_nets(&gclk, &gclk_in, &mut wires);
    let gclk_source = endpoint_source_nets(&gclk, &gclk_out, &mut wires);

    assert_eq!(wires.resolve(iob_source[0]), "LEFT_I7");
    assert_eq!(wires.resolve(iob_sink[0]), "LEFT_O7");
    assert_eq!(wires.resolve(gclk_sink[0]), "CLKC_GCLK2");
    assert_eq!(wires.resolve(gclk_source[0]), "CLKC_GCLK2_PW");
}

#[test]
fn parses_external_transmission_graphs_when_resources_exist() {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return;
    };
    let arch = bundle.root.join("fdp3p7_arch.xml");
    let cil = bundle.root.join("fdp3p7_cil.xml");
    if !arch.exists() || !cil.exists() {
        return;
    }
    let cil = load_cil(&cil).expect("load cil");
    let mut wires = WireInterner::default();
    let graphs = load_site_route_graphs(&arch, &cil, &mut wires).expect("load route graphs");
    let center = graphs.get("GSB_CNT").expect("center graph");
    let w9 = wires.id("W9").expect("W9 wire");
    let e9 = wires.id("E9").expect("E9 wire");
    assert!(
        center
            .adjacency
            .get(&w9)
            .is_some_and(|indices| indices.iter().any(|index| center.arcs[*index].to == e9))
    );
    let n7 = wires.id("N7").expect("N7 wire");
    let np7 = wires.id("N_P7").expect("N_P7 wire");
    assert!(
        center
            .adjacency
            .get(&n7)
            .is_some_and(|indices| indices.iter().any(|index| center.arcs[*index].to == np7))
    );
    let left = graphs.get("GSB_LFT").expect("left graph");
    let left_i1 = wires.id("LEFT_I1").expect("LEFT_I1 wire");
    let left_e10 = wires.id("LEFT_E10").expect("LEFT_E10 wire");
    assert!(
        left.adjacency.get(&left_i1).is_some_and(|indices| {
            indices.iter().any(|index| left.arcs[*index].to == left_e10)
        })
    );
}

#[test]
fn clock_spine_stitching_reaches_center_and_clkv_tiles() {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return;
    };
    let arch = bundle.root.join("fdp3p7_arch.xml");
    if !arch.exists() {
        return;
    }
    let arch = load_arch(&arch).expect("load arch");
    let mut wires = WireInterner::default();

    let from_clkb = RouteNode::new(34, 27, wires.intern("CLKB_GCLK0"));
    let clkb_neighbors = clock_spine_neighbors(&arch, &mut wires, &from_clkb);
    assert!(
        clkb_neighbors
            .iter()
            .any(|&(x, y, wire)| x == 17 && y == 27 && wires.resolve(wire) == "CLKC_GCLK0")
    );

    let from_clkc = RouteNode::new(17, 27, wires.intern("CLKC_VGCLK0"));
    let clkc_neighbors = clock_spine_neighbors(&arch, &mut wires, &from_clkc);
    assert!(
        clkc_neighbors
            .iter()
            .any(|&(x, y, wire)| x == 16 && y == 27 && wires.resolve(wire) == "CLKV_VGCLK0")
    );

    let from_clkv = RouteNode::new(16, 27, wires.intern("CLKV_GCLK_BUFL0"));
    let clkv_neighbors = clock_spine_neighbors(&arch, &mut wires, &from_clkv);
    assert!(
        clkv_neighbors
            .iter()
            .any(|&(x, y, wire)| x == 16 && y == 26 && wires.resolve(wire) == "GCLK0")
    );
}
