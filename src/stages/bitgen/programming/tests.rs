use super::{
    build_programming_image,
    derive::derive_site_programs,
    types::{
        RequestedConfig, SiteProgramKind, SliceClockEnableMode, SliceFfDataPath,
        SliceLutOutputUsage,
    },
};
use crate::{
    bitgen::DeviceDesignIndex,
    bitgen::{DeviceCell, DeviceDesign, DeviceEndpoint, DeviceNet},
    cil::parse_cil_str,
    domain::{CellKind, NetOrigin, SiteKind},
    ir::Property,
};

fn logic_slice_program(device: &DeviceDesign) -> super::types::SliceProgram {
    let index = DeviceDesignIndex::build(device);
    derive_site_programs(device, &index)
        .into_iter()
        .find_map(|site| match site.kind {
            SiteProgramKind::LogicSlice(program) => Some(program),
            SiteProgramKind::Iob(_) | SiteProgramKind::Gclk | SiteProgramKind::GclkIob => None,
        })
        .expect("logic slice program")
}

fn compiled_logic_slice_requests(device: &DeviceDesign, cil_xml: &str) -> Vec<RequestedConfig> {
    let cil = parse_cil_str(cil_xml).expect("parse mini cil");
    build_programming_image(device, &cil, None)
        .sites
        .into_iter()
        .find(|site| site.site_kind == SiteKind::LogicSlice)
        .expect("compiled logic slice site")
        .requests
}

fn mini_logic_slice_lut_cil() -> &'static str {
    r##"
        <device name="mini">
          <site_library>
            <block_site name="SLICE">
              <config_info amount="1">
                <cfg_element name="F">
                  <function name="0xFFFF" quomodo="srambit" manner="computation" default="no">
                    <sram basic_cell="FLUT0" name="SRAM" address="0"/>
                    <sram basic_cell="FLUT1" name="SRAM" address="1"/>
                    <sram basic_cell="FLUT2" name="SRAM" address="2"/>
                    <sram basic_cell="FLUT3" name="SRAM" address="3"/>
                    <sram basic_cell="FLUT4" name="SRAM" address="4"/>
                    <sram basic_cell="FLUT5" name="SRAM" address="5"/>
                    <sram basic_cell="FLUT6" name="SRAM" address="6"/>
                    <sram basic_cell="FLUT7" name="SRAM" address="7"/>
                    <sram basic_cell="FLUT8" name="SRAM" address="8"/>
                    <sram basic_cell="FLUT9" name="SRAM" address="9"/>
                    <sram basic_cell="FLUT10" name="SRAM" address="10"/>
                    <sram basic_cell="FLUT11" name="SRAM" address="11"/>
                    <sram basic_cell="FLUT12" name="SRAM" address="12"/>
                    <sram basic_cell="FLUT13" name="SRAM" address="13"/>
                    <sram basic_cell="FLUT14" name="SRAM" address="14"/>
                    <sram basic_cell="FLUT15" name="SRAM" address="15"/>
                  </function>
                </cfg_element>
              </config_info>
            </block_site>
          </site_library>
        </device>
        "##
}

#[test]
fn detects_local_lut_ff_data_path_for_paired_driver() {
    let lut0 = DeviceCell::new("lut0", CellKind::Lut, "LUT4")
        .with_properties(vec![Property::new("lut_init", "0xA")])
        .placed(
            SiteKind::LogicSlice,
            "S0",
            "LUT0",
            "T0",
            "CENTER",
            (0, 0, 0),
        );
    let ff0 = DeviceCell::new("ff0", CellKind::Ff, "DFFHQ").placed(
        SiteKind::LogicSlice,
        "S0",
        "FF0",
        "T0",
        "CENTER",
        (0, 0, 0),
    );
    let device = DeviceDesign {
        cells: vec![lut0, ff0],
        nets: vec![
            DeviceNet::new("n0", NetOrigin::Logical)
                .with_driver(DeviceEndpoint::cell("lut0", "O", (0, 0, 0)))
                .with_sink(DeviceEndpoint::cell("ff0", "D", (0, 0, 0))),
        ],
        ..DeviceDesign::default()
    };

    let slice = logic_slice_program(&device);
    assert_eq!(
        slice.slots[0].ff.as_ref().expect("slot0 ff").data_path,
        SliceFfDataPath::LocalLut
    );
}

#[test]
fn detects_site_bypass_for_nonlocal_ff_driver() {
    let ff0 = DeviceCell::new("ff0", CellKind::Ff, "DFFHQ").placed(
        SiteKind::LogicSlice,
        "S0",
        "FF0",
        "T0",
        "CENTER",
        (0, 0, 0),
    );
    let other = DeviceCell::new("lut1", CellKind::Lut, "LUT4")
        .with_properties(vec![Property::new("lut_init", "0xA")])
        .placed(
            SiteKind::LogicSlice,
            "S1",
            "LUT1",
            "T0",
            "CENTER",
            (0, 0, 1),
        );
    let device = DeviceDesign {
        cells: vec![ff0, other],
        nets: vec![
            DeviceNet::new("n0", NetOrigin::Logical)
                .with_driver(DeviceEndpoint::cell("lut1", "O", (0, 0, 1)))
                .with_sink(DeviceEndpoint::cell("ff0", "D", (0, 0, 0))),
        ],
        ..DeviceDesign::default()
    };

    let slice = logic_slice_program(&device);
    assert_eq!(
        slice.slots[0].ff.as_ref().expect("slot0 ff").data_path,
        SliceFfDataPath::SiteBypass
    );
}

#[test]
fn classifies_hidden_vs_routed_lut_outputs_before_encoding() {
    let lut0 = DeviceCell::new("lut0", CellKind::Lut, "LUT4")
        .with_properties(vec![Property::new("lut_init", "0xA")])
        .placed(
            SiteKind::LogicSlice,
            "S0",
            "LUT0",
            "T0",
            "CENTER",
            (0, 0, 0),
        );
    let ff0 = DeviceCell::new("ff0", CellKind::Ff, "DFFHQ").placed(
        SiteKind::LogicSlice,
        "S0",
        "FF0",
        "T0",
        "CENTER",
        (0, 0, 0),
    );
    let hidden_only = DeviceDesign {
        cells: vec![lut0.clone(), ff0.clone()],
        nets: vec![
            DeviceNet::new("n0", NetOrigin::Logical)
                .with_driver(DeviceEndpoint::cell("lut0", "O", (0, 0, 0)))
                .with_sink(DeviceEndpoint::cell("ff0", "D", (0, 0, 0))),
        ],
        ..DeviceDesign::default()
    };
    let routed = DeviceDesign {
        cells: vec![lut0, ff0],
        nets: vec![
            DeviceNet::new("n0", NetOrigin::Logical)
                .with_driver(DeviceEndpoint::cell("lut0", "O", (0, 0, 0)))
                .with_sink(DeviceEndpoint::cell("ff0", "D", (0, 0, 0))),
            DeviceNet::new("n1", NetOrigin::Logical)
                .with_driver(DeviceEndpoint::cell("lut0", "O", (0, 0, 0)))
                .with_sink(DeviceEndpoint::port("out", "OUT", (1, 1, 0))),
        ],
        ..DeviceDesign::default()
    };

    assert_eq!(
        logic_slice_program(&hidden_only).slots[0]
            .lut
            .as_ref()
            .expect("slot0 lut")
            .output_usage,
        SliceLutOutputUsage::HiddenLocalOnly
    );
    assert_eq!(
        logic_slice_program(&routed).slots[0]
            .lut
            .as_ref()
            .expect("slot0 lut")
            .output_usage,
        SliceLutOutputUsage::RoutedOutput
    );
}

#[test]
fn detects_clock_enable_usage_in_site_program() {
    let ff0 = DeviceCell::new("ff0", CellKind::Ff, "DFFHQ").placed(
        SiteKind::LogicSlice,
        "S0",
        "FF0",
        "T0",
        "CENTER",
        (0, 0, 0),
    );
    let device = DeviceDesign {
        cells: vec![ff0],
        nets: vec![
            DeviceNet::new("ce", NetOrigin::Logical)
                .with_driver(DeviceEndpoint::port("ce_in", "IN", (1, 0, 0)))
                .with_sink(DeviceEndpoint::cell("ff0", "CE", (0, 0, 0))),
        ],
        ..DeviceDesign::default()
    };

    let slice = logic_slice_program(&device);
    assert_eq!(slice.clock_enable_mode, SliceClockEnableMode::DirectCe);
}

#[test]
fn widens_small_lut_init_to_site_truth_table_width_before_encoding() {
    let device = DeviceDesign {
        cells: vec![
            DeviceCell::new("lut0", CellKind::Lut, "LUT2")
                .with_properties(vec![Property::new("lut_init", "1")])
                .placed(
                    SiteKind::LogicSlice,
                    "S0",
                    "LUT0",
                    "T0",
                    "CENTER",
                    (0, 0, 0),
                ),
        ],
        ..DeviceDesign::default()
    };

    let requests = compiled_logic_slice_requests(&device, mini_logic_slice_lut_cil());

    assert!(
        requests
            .iter()
            .any(|request| request.cfg_name == "F" && request.function_name == "0x1111")
    );
}

#[test]
fn prefers_raw_init_over_canonical_lut_init_when_both_are_present() {
    let device = DeviceDesign {
        cells: vec![
            DeviceCell::new("lut0", CellKind::Lut, "LUT2")
                .with_properties(vec![
                    Property::new("init", "12"),
                    Property::new("lut_init", "0xC"),
                ])
                .placed(
                    SiteKind::LogicSlice,
                    "S0",
                    "LUT0",
                    "T0",
                    "CENTER",
                    (0, 0, 0),
                ),
        ],
        ..DeviceDesign::default()
    };

    let requests = compiled_logic_slice_requests(&device, mini_logic_slice_lut_cil());

    assert!(
        requests
            .iter()
            .any(|request| request.cfg_name == "F" && request.function_name == "0x1212")
    );
}

#[test]
fn programming_boundary_accepts_raw_init_without_canonical_lut_init() {
    let device = DeviceDesign {
        cells: vec![
            DeviceCell::new("lut0", CellKind::Lut, "LUT2")
                .with_properties(vec![Property::new("init", "12")])
                .placed(
                    SiteKind::LogicSlice,
                    "S0",
                    "LUT0",
                    "T0",
                    "CENTER",
                    (0, 0, 0),
                ),
        ],
        ..DeviceDesign::default()
    };

    let requests = compiled_logic_slice_requests(&device, mini_logic_slice_lut_cil());

    assert!(
        requests
            .iter()
            .any(|request| request.cfg_name == "F" && request.function_name == "0x1212")
    );
}

#[test]
fn normalized_lut1_prefers_raw_decimal_init_compact_semantics() {
    let device = DeviceDesign {
        cells: vec![
            DeviceCell::new("lut0", CellKind::Lut, "LUT1")
                .with_properties(vec![
                    Property::new("init", "15"),
                    Property::new("lut_init", "0x3"),
                    Property::new("pin_map_ADR0", "0,1"),
                ])
                .placed(
                    SiteKind::LogicSlice,
                    "S0",
                    "LUT0",
                    "T0",
                    "CENTER",
                    (0, 0, 0),
                ),
        ],
        ..DeviceDesign::default()
    };

    let requests = compiled_logic_slice_requests(&device, mini_logic_slice_lut_cil());

    assert!(
        requests
            .iter()
            .any(|request| request.cfg_name == "F" && request.function_name == "0x1515")
    );
}

#[test]
fn routed_lut_only_slice_emits_usage_bits_without_ff_controls() {
    let device = DeviceDesign {
        cells: vec![
            DeviceCell::new("lut0", CellKind::Lut, "LUT2")
                .with_properties(vec![Property::new("lut_init", "0x5")])
                .placed(
                    SiteKind::LogicSlice,
                    "S0",
                    "LUT0",
                    "T0",
                    "CENTER",
                    (0, 0, 0),
                ),
            DeviceCell::new("lut1", CellKind::Lut, "LUT2")
                .with_properties(vec![Property::new("lut_init", "0xA")])
                .placed(
                    SiteKind::LogicSlice,
                    "S0",
                    "LUT1",
                    "T0",
                    "CENTER",
                    (0, 0, 0),
                ),
        ],
        nets: vec![
            DeviceNet::new("n0", NetOrigin::Logical)
                .with_driver(DeviceEndpoint::cell("lut0", "O", (0, 0, 0)))
                .with_sink(DeviceEndpoint::port("out0", "OUT", (1, 0, 0))),
            DeviceNet::new("n1", NetOrigin::Logical)
                .with_driver(DeviceEndpoint::cell("lut1", "O", (0, 0, 0)))
                .with_sink(DeviceEndpoint::port("out1", "OUT", (1, 1, 0))),
        ],
        ..DeviceDesign::default()
    };

    let requests = compiled_logic_slice_requests(
        &device,
        r##"
        <device name="mini">
          <site_library>
            <block_site name="SLICE">
              <config_info amount="2">
                <cfg_element name="F">
                  <function name="0x0" quomodo="srambit" manner="computation" default="no">
                    <sram basic_cell="FLUT0" name="SRAM" address="0"/>
                    <sram basic_cell="FLUT1" name="SRAM" address="1"/>
                    <sram basic_cell="FLUT2" name="SRAM" address="2"/>
                    <sram basic_cell="FLUT3" name="SRAM" address="3"/>
                  </function>
                </cfg_element>
                <cfg_element name="G">
                  <function name="0x0" quomodo="srambit" manner="computation" default="no">
                    <sram basic_cell="GLUT0" name="SRAM" address="0"/>
                    <sram basic_cell="GLUT1" name="SRAM" address="1"/>
                    <sram basic_cell="GLUT2" name="SRAM" address="2"/>
                    <sram basic_cell="GLUT3" name="SRAM" address="3"/>
                  </function>
                </cfg_element>
              </config_info>
            </block_site>
          </site_library>
        </device>
        "##,
    );

    assert!(
        requests
            .iter()
            .any(|request| request.cfg_name == "XUSED" && request.function_name == "0")
    );
    assert!(
        requests
            .iter()
            .any(|request| request.cfg_name == "YUSED" && request.function_name == "0")
    );
    assert!(
        requests
            .iter()
            .any(|request| request.cfg_name == "FXMUX" && request.function_name == "F")
    );
    assert!(
        requests
            .iter()
            .any(|request| request.cfg_name == "GYMUX" && request.function_name == "G")
    );
    assert!(
        !requests
            .iter()
            .any(|request| matches!(request.cfg_name.as_str(), "DXMUX" | "DYMUX" | "CKINV"))
    );
}
