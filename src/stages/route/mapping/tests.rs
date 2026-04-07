use super::{endpoint_sink_nets, endpoint_source_nets, should_skip_unmapped_sink};
use crate::{
    bitgen::{DeviceCell, DeviceEndpoint},
    domain::{CellKind, EndpointKind, SiteKind},
};

#[test]
fn clock_sites_map_pad_mux_and_buffer_wires_distinctly() {
    let mut wires = super::super::types::WireInterner::default();
    let gclkiob = DeviceCell::new("clkpad", CellKind::Io, "IOB").placed(
        SiteKind::GclkIob,
        "GCLKIOB1",
        "PAD",
        "R34C27",
        "CLKB",
        (34, 27, 1),
    );
    let gclk = DeviceCell::new("gclk", CellKind::GlobalClockBuffer, "GCLK").placed(
        SiteKind::Gclk,
        "GCLKBUF1",
        "BUF",
        "R34C27",
        "CLKB",
        (34, 27, 1),
    );

    let gclkiob_out = DeviceEndpoint::new(EndpointKind::Cell, "clkpad", "GCLKOUT", (34, 27, 1));
    let gclk_in = DeviceEndpoint::new(EndpointKind::Cell, "gclk", "IN", (34, 27, 1));
    let gclk_out = DeviceEndpoint::new(EndpointKind::Cell, "gclk", "OUT", (34, 27, 1));

    let pad_source = endpoint_source_nets(&gclkiob, &gclkiob_out, &mut wires);
    let gclk_sink = endpoint_sink_nets(None, &gclk, &gclk_in, &mut wires);
    let gclk_source = endpoint_source_nets(&gclk, &gclk_out, &mut wires);

    assert_eq!(wires.resolve(pad_source[0]), "CLKB_CLKPAD1");
    assert_eq!(wires.resolve(gclk_sink[0]), "CLKB_GCLKBUF1_IN");
    assert_eq!(wires.resolve(gclk_source[0]), "CLKB_GCLK1_PW");
}

#[test]
fn register_data_routes_through_bx_by_when_not_driven_by_paired_lut() {
    let mut wires = super::super::types::WireInterner::default();
    let ff0 = DeviceCell::new("ff0", CellKind::Ff, "DFFHQ").placed(
        SiteKind::LogicSlice,
        "S0",
        "FF0",
        "R5C47",
        "CENTER",
        (5, 47, 0),
    );
    let ff1 = DeviceCell::new("ff1", CellKind::Ff, "DFFHQ").placed(
        SiteKind::LogicSlice,
        "S0",
        "FF1",
        "R5C47",
        "CENTER",
        (5, 47, 0),
    );
    let lut0 = DeviceCell::new("lut0", CellKind::Lut, "LUT4").placed(
        SiteKind::LogicSlice,
        "S0",
        "LUT0",
        "R5C47",
        "CENTER",
        (5, 47, 0),
    );
    let other = DeviceCell::new("other", CellKind::Lut, "LUT4").placed(
        SiteKind::LogicSlice,
        "S1",
        "LUT1",
        "R5C47",
        "CENTER",
        (5, 47, 1),
    );
    let d = DeviceEndpoint::new(EndpointKind::Cell, "ff0", "D", (5, 47, 0));
    let d1 = DeviceEndpoint::new(EndpointKind::Cell, "ff1", "D", (5, 47, 0));

    let local = endpoint_sink_nets(Some(&lut0), &ff0, &d, &mut wires);
    let routed_x = endpoint_sink_nets(Some(&other), &ff0, &d, &mut wires);
    let routed_y = endpoint_sink_nets(None, &ff1, &d1, &mut wires);

    assert!(local.is_empty());
    assert!(should_skip_unmapped_sink(Some(&lut0), &ff0, &d));
    assert_eq!(wires.resolve(routed_x[0]), "S0_BX_B");
    assert!(!should_skip_unmapped_sink(Some(&other), &ff0, &d));
    assert_eq!(wires.resolve(routed_y[0]), "S0_BY_B");
    assert!(!should_skip_unmapped_sink(None, &ff1, &d1));
}

#[test]
fn edff_enable_pin_routes_through_slice_ce_wire() {
    let mut wires = super::super::types::WireInterner::default();
    let edff = DeviceCell::new("ff0", CellKind::Ff, "EDFFHQ").placed(
        SiteKind::LogicSlice,
        "S0",
        "FF0",
        "R5C47",
        "CENTER",
        (5, 47, 0),
    );
    let e = DeviceEndpoint::new(EndpointKind::Cell, "ff0", "E", (5, 47, 0));

    let routed = endpoint_sink_nets(None, &edff, &e, &mut wires);

    assert_eq!(routed.len(), 1);
    assert_eq!(wires.resolve(routed[0]), "S0_CE_B");
}

#[test]
fn compacted_lut_inputs_expand_back_to_their_physical_pins() {
    let mut wires = super::super::types::WireInterner::default();
    let lut = DeviceCell::new("lut0", CellKind::Lut, "LUT1")
        .with_properties(vec![crate::ir::Property::new("pin_map_ADR0", "0,1")])
        .placed(
            SiteKind::LogicSlice,
            "S0",
            "LUT0",
            "R5C47",
            "CENTER",
            (5, 47, 0),
        );
    let a0 = DeviceEndpoint::new(EndpointKind::Cell, "lut0", "ADR0", (5, 47, 0));

    let routed = endpoint_sink_nets(None, &lut, &a0, &mut wires);

    assert_eq!(routed.len(), 2);
    assert_eq!(wires.resolve(routed[0]), "S0_F_B1");
    assert_eq!(wires.resolve(routed[1]), "S0_F_B2");
    assert!(super::sink_requires_all_wires(&lut, &a0));
}

#[test]
fn block_ram_endpoints_map_to_cpp_compatible_bram_wires() {
    let mut wires = super::super::types::WireInterner::default();
    let bram = DeviceCell::new("ram0", CellKind::BlockRam, "BLOCKRAM_2").placed(
        SiteKind::BlockRam,
        "BRAM",
        "BRAM",
        "LBRAMR12C0",
        "LBRAMD",
        (12, 5, 0),
    );
    let dia0 = DeviceEndpoint::new(EndpointKind::Cell, "ram0", "DIA0", (9, 5, 0));
    let enb = DeviceEndpoint::new(EndpointKind::Cell, "ram0", "ENB", (11, 5, 0));
    let dob15 = DeviceEndpoint::new(EndpointKind::Cell, "ram0", "DOB15", (12, 5, 0));

    let input = endpoint_sink_nets(None, &bram, &dia0, &mut wires);
    let enable = endpoint_sink_nets(None, &bram, &enb, &mut wires);
    let output = endpoint_source_nets(&bram, &dob15, &mut wires);

    assert_eq!(wires.resolve(input[0]), "BRAM_DIA0");
    assert_eq!(wires.resolve(enable[0]), "BRAM_SELB");
    assert_eq!(wires.resolve(output[0]), "BRAM_DOB15");
}
