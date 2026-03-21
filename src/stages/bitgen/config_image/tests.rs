use super::build_config_image;
use crate::{
    cil::parse_cil_str,
    device::{DeviceCell, DeviceDesign, DeviceEndpoint, DeviceNet},
    domain::{EndpointKind, SiteKind},
    ir::{PortDirection, Property},
    resource::{Arch, TileInstance},
};
use std::collections::BTreeMap;

#[test]
fn builds_tile_assignments_for_slice_iob_and_gclk() {
    let cil = parse_cil_str(
        r##"
        <device name="mini">
          <site_library>
            <block_site name="SLICE">
              <config_info amount="6">
                <cfg_element name="F">
                  <function name="0x0" quomodo="srambit" manner="computation" default="no">
                    <sram basic_cell="FLUT0" name="SRAM" address="0"/>
                    <sram basic_cell="FLUT1" name="SRAM" address="1"/>
                    <sram basic_cell="FLUT2" name="SRAM" address="2"/>
                    <sram basic_cell="FLUT3" name="SRAM" address="3"/>
                  </function>
                </cfg_element>
                <cfg_element name="FFX">
                  <function name="#FF" default="yes">
                    <sram basic_cell="FFX" name="LF" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="INITX">
                  <function name="LOW" default="no">
                    <sram basic_cell="FFX" name="INIT" content="0"/>
                  </function>
                </cfg_element>
                <cfg_element name="SYNCX">
                  <function name="ASYNC" default="no">
                    <sram basic_cell="FFX" name="AS" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="DXMUX">
                  <function name="1" default="yes">
                    <sram basic_cell="DXMUX" name="S0" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="DYMUX">
                  <function name="1" default="yes">
                    <sram basic_cell="DYMUX" name="S0" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="FXMUX">
                  <function name="F" default="no">
                    <sram basic_cell="FXMUX" name="S0" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="CKINV">
                  <function name="1" default="no">
                    <sram basic_cell="CKINV" name="P" content="1"/>
                  </function>
                </cfg_element>
              </config_info>
            </block_site>
            <block_site name="IOB">
              <config_info amount="3">
                <cfg_element name="IOATTRBOX">
                  <function name="LVTTL" default="yes"/>
                </cfg_element>
                <cfg_element name="OMUX">
                  <function name="O" default="no">
                    <sram basic_cell="OMUX" name="S0" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="OUTMUX">
                  <function name="1" default="no">
                    <sram basic_cell="OUTMUX" name="S0" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="DRIVEATTRBOX">
                  <function name="#OFF" default="yes">
                    <sram basic_cell="TRIMUX" name="S0" content="1"/>
                  </function>
                  <function name="12" default="no">
                    <sram basic_cell="TRIMUX" name="S0" content="1"/>
                  </function>
                </cfg_element>
              </config_info>
            </block_site>
            <block_site name="GCLK">
              <config_info amount="2">
                <cfg_element name="CEMUX">
                  <function name="1" default="no">
                    <sram basic_cell="CEMUX" name="P0" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="DISABLE_ATTR">
                  <function name="LOW" default="no">
                    <sram basic_cell="CKGATE" name="P0" content="0"/>
                  </function>
                </cfg_element>
              </config_info>
            </block_site>
          </site_library>
          <cluster_library>
            <homogeneous_cluster name="SLICE1x1" type="SLICE"/>
            <homogeneous_cluster name="IOB1x1" type="IOB"/>
            <homogeneous_cluster name="GCLK1x1" type="GCLK"/>
          </cluster_library>
          <tile_library>
            <tile name="CENTER" sram_amount="R4C8">
              <cluster_info amount="1">
                <cluster type="SLICE1x1">
                  <site name="S0" position="R0C0">
                    <site_sram>
                      <sram basic_cell="FLUT0" sram_name="SRAM" local_place="B0W0"/>
                      <sram basic_cell="FLUT1" sram_name="SRAM" local_place="B0W1"/>
                      <sram basic_cell="FLUT2" sram_name="SRAM" local_place="B0W2"/>
                      <sram basic_cell="FLUT3" sram_name="SRAM" local_place="B0W3"/>
                      <sram basic_cell="FFX" sram_name="LF" local_place="B1W0"/>
                      <sram basic_cell="FFX" sram_name="INIT" local_place="B1W1"/>
                      <sram basic_cell="FFX" sram_name="AS" local_place="B1W2"/>
                      <sram basic_cell="DXMUX" sram_name="S0" local_place="B1W3"/>
                      <sram basic_cell="DYMUX" sram_name="S0" local_place="B2W3"/>
                      <sram basic_cell="FXMUX" sram_name="S0" local_place="B2W0"/>
                      <sram basic_cell="CKINV" sram_name="P" local_place="B2W1"/>
                    </site_sram>
                  </site>
                </cluster>
              </cluster_info>
            </tile>
            <tile name="RIGHT" sram_amount="R2C4">
              <cluster_info amount="1">
                <cluster type="IOB1x1">
                  <site name="IOB0" position="R0C0">
                    <site_sram>
                      <sram basic_cell="OMUX" sram_name="S0" local_place="B0W0"/>
                      <sram basic_cell="TRIMUX" sram_name="S0" local_place="B0W1"/>
                      <sram basic_cell="OUTMUX" sram_name="S0" local_place="B0W2"/>
                    </site_sram>
                  </site>
                </cluster>
              </cluster_info>
            </tile>
            <tile name="CLKB" sram_amount="R2C4">
              <cluster_info amount="1">
                <cluster type="GCLK1x1">
                  <site name="GCLKBUF0" position="R0C0">
                    <site_sram>
                      <sram basic_cell="CEMUX" sram_name="P0" local_place="B0W0"/>
                      <sram basic_cell="CKGATE" sram_name="P0" local_place="B0W1"/>
                    </site_sram>
                  </site>
                </cluster>
              </cluster_info>
            </tile>
          </tile_library>
        </device>
        "##,
    )
    .expect("parse mini cil");

    let device = DeviceDesign {
        name: "mini".to_string(),
        device: "mini".to_string(),
        ports: vec![],
        cells: vec![
            DeviceCell {
                cell_name: "u_lut".to_string(),
                type_name: "LUT2".to_string(),
                properties: vec![Property::new("lut_init", "10")],
                site_kind: SiteKind::LogicSlice,
                site_name: "S0".to_string(),
                bel: "LUT0".to_string(),
                tile_name: "T0".to_string(),
                tile_type: "CENTER".to_string(),
                x: 1,
                y: 1,
                z: 0,
                cluster_name: Some("clb_0".to_string()),
                synthetic: false,
            },
            DeviceCell {
                cell_name: "u_ff".to_string(),
                type_name: "DFFHQ".to_string(),
                properties: Vec::new(),
                site_kind: SiteKind::LogicSlice,
                site_name: "S0".to_string(),
                bel: "FF0".to_string(),
                tile_name: "T0".to_string(),
                tile_type: "CENTER".to_string(),
                x: 1,
                y: 1,
                z: 0,
                cluster_name: Some("clb_0".to_string()),
                synthetic: false,
            },
            DeviceCell {
                cell_name: "$iob$out".to_string(),
                type_name: "IOB".to_string(),
                properties: Vec::new(),
                site_kind: SiteKind::Iob,
                site_name: "IOB0".to_string(),
                bel: "PAD".to_string(),
                tile_name: "IO0".to_string(),
                tile_type: "RIGHT".to_string(),
                x: 3,
                y: 0,
                z: 0,
                cluster_name: None,
                synthetic: true,
            },
            DeviceCell {
                cell_name: "$gclk$clk".to_string(),
                type_name: "GCLK".to_string(),
                properties: Vec::new(),
                site_kind: SiteKind::Gclk,
                site_name: "GCLKBUF0".to_string(),
                bel: "BUF".to_string(),
                tile_name: "CLK0".to_string(),
                tile_type: "CLKB".to_string(),
                x: 4,
                y: 0,
                z: 0,
                cluster_name: None,
                synthetic: true,
            },
        ],
        nets: vec![DeviceNet {
            name: "logic_to_out".to_string(),
            driver: Some(DeviceEndpoint {
                kind: EndpointKind::Cell,
                name: "u_ff".to_string(),
                pin: "Q".to_string(),
                x: 1,
                y: 1,
                z: 0,
            }),
            sinks: vec![DeviceEndpoint {
                kind: EndpointKind::Cell,
                name: "$iob$out".to_string(),
                pin: "OUT".to_string(),
                x: 3,
                y: 0,
                z: 0,
            }],
            origin: "logical-net".into(),
            guide_tiles: Vec::new(),
            sink_guides: Vec::new(),
        }],
        notes: vec![PortDirection::Output.as_str().to_string()],
    };

    let image = build_config_image(&device, &cil, None, None).expect("build config image");
    assert_eq!(image.tiles.len(), 3);

    let center = image
        .tiles
        .iter()
        .find(|tile| tile.tile_name == "T0")
        .expect("center tile");
    assert!(
        center
            .configs
            .iter()
            .any(|cfg| cfg.cfg_name == "F" && cfg.function_name == "0xA")
    );
    assert!(
        center
            .assignments
            .iter()
            .any(|bit| bit.basic_cell == "FLUT1" && bit.value == 1)
    );
    assert!(
        center
            .assignments
            .iter()
            .any(|bit| bit.basic_cell == "FLUT3" && bit.value == 1)
    );
    assert!(
        center
            .assignments
            .iter()
            .any(|bit| bit.cfg_name == "FFX" && bit.value == 1)
    );
    assert!(
        center
            .assignments
            .iter()
            .any(|bit| bit.cfg_name == "SYNCX" && bit.value == 1)
    );
    assert!(
        !center
            .assignments
            .iter()
            .any(|bit| bit.basic_cell == "DYMUX")
    );
    assert!(center.packed_bits().iter().any(|byte| *byte != 0));

    let right = image
        .tiles
        .iter()
        .find(|tile| tile.tile_name == "IO0")
        .expect("iob tile");
    assert!(
        right
            .configs
            .iter()
            .any(|cfg| cfg.cfg_name == "OMUX" && cfg.function_name == "O")
    );
    assert!(
        right
            .assignments
            .iter()
            .any(|bit| bit.basic_cell == "OMUX" && bit.value == 1)
    );
    assert!(
        right
            .assignments
            .iter()
            .any(|bit| bit.basic_cell == "OUTMUX" && bit.value == 1)
    );

    let clkb = image
        .tiles
        .iter()
        .find(|tile| tile.tile_name == "CLK0")
        .expect("gclk tile");
    assert!(
        clkb.configs
            .iter()
            .any(|cfg| cfg.cfg_name == "CEMUX" && cfg.function_name == "1")
    );
    assert!(
        clkb.assignments
            .iter()
            .any(|bit| bit.basic_cell == "CEMUX" && bit.value == 1)
    );
}

#[test]
fn widens_small_lut_init_to_site_truth_table_width() {
    let cil = parse_cil_str(
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
          <cluster_library>
            <homogeneous_cluster name="SLICE1x1" type="SLICE"/>
          </cluster_library>
          <tile_library>
            <tile name="CENTER" sram_amount="R4C16">
              <cluster_info amount="1">
                <cluster type="SLICE1x1">
                  <site name="S0" position="R0C0">
                    <site_sram>
                      <sram basic_cell="FLUT0" sram_name="SRAM" local_place="B0W0"/>
                      <sram basic_cell="FLUT1" sram_name="SRAM" local_place="B0W1"/>
                      <sram basic_cell="FLUT2" sram_name="SRAM" local_place="B0W2"/>
                      <sram basic_cell="FLUT3" sram_name="SRAM" local_place="B0W3"/>
                      <sram basic_cell="FLUT4" sram_name="SRAM" local_place="B0W4"/>
                      <sram basic_cell="FLUT5" sram_name="SRAM" local_place="B0W5"/>
                      <sram basic_cell="FLUT6" sram_name="SRAM" local_place="B0W6"/>
                      <sram basic_cell="FLUT7" sram_name="SRAM" local_place="B0W7"/>
                      <sram basic_cell="FLUT8" sram_name="SRAM" local_place="B0W8"/>
                      <sram basic_cell="FLUT9" sram_name="SRAM" local_place="B0W9"/>
                      <sram basic_cell="FLUT10" sram_name="SRAM" local_place="B0W10"/>
                      <sram basic_cell="FLUT11" sram_name="SRAM" local_place="B0W11"/>
                      <sram basic_cell="FLUT12" sram_name="SRAM" local_place="B0W12"/>
                      <sram basic_cell="FLUT13" sram_name="SRAM" local_place="B0W13"/>
                      <sram basic_cell="FLUT14" sram_name="SRAM" local_place="B0W14"/>
                      <sram basic_cell="FLUT15" sram_name="SRAM" local_place="B0W15"/>
                    </site_sram>
                  </site>
                </cluster>
              </cluster_info>
            </tile>
          </tile_library>
        </device>
        "##,
    )
    .expect("parse mini cil");

    let device = DeviceDesign {
        name: "mini".to_string(),
        device: "mini".to_string(),
        ports: vec![],
        cells: vec![DeviceCell {
            cell_name: "u_lut".to_string(),
            type_name: "LUT2".to_string(),
            properties: vec![Property::new("lut_init", "1")],
            site_kind: SiteKind::LogicSlice,
            site_name: "S0".to_string(),
            bel: "LUT0".to_string(),
            tile_name: "T0".to_string(),
            tile_type: "CENTER".to_string(),
            x: 1,
            y: 1,
            z: 0,
            cluster_name: Some("clb_0".to_string()),
            synthetic: false,
        }],
        nets: vec![],
        notes: vec![],
    };

    let image = build_config_image(&device, &cil, None, None).expect("build config image");
    let center = image
        .tiles
        .iter()
        .find(|tile| tile.tile_name == "T0")
        .expect("center tile");

    assert!(
        center
            .configs
            .iter()
            .any(|cfg| cfg.cfg_name == "F" && cfg.function_name == "0x1111")
    );
    for basic_cell in ["FLUT0", "FLUT4", "FLUT8", "FLUT12"] {
        assert!(
            center
                .assignments
                .iter()
                .any(|bit| bit.basic_cell == basic_cell && bit.value == 1)
        );
    }
    for basic_cell in ["FLUT1", "FLUT2", "FLUT3", "FLUT5"] {
        assert!(
            !center
                .assignments
                .iter()
                .any(|bit| bit.basic_cell == basic_cell && bit.value == 1)
        );
    }
}

#[test]
fn owner_tile_assignments_move_into_target_tile_when_arch_is_available() {
    let cil = parse_cil_str(
        r##"
        <device name="mini">
          <site_library>
            <block_site name="SLICE">
              <config_info amount="1">
                <cfg_element name="FFX">
                  <function name="#FF" default="yes">
                    <sram basic_cell="FFX" name="LF" content="1"/>
                  </function>
                </cfg_element>
              </config_info>
            </block_site>
          </site_library>
          <cluster_library>
            <homogeneous_cluster name="SLICE1x1" type="SLICE"/>
          </cluster_library>
          <tile_library>
            <tile name="OWNER" sram_amount="R1C1"/>
            <tile name="SOURCE" sram_amount="R1C1">
              <cluster_info amount="1">
                <cluster type="SLICE1x1">
                  <site name="S0" position="R0C0">
                    <site_sram>
                      <sram basic_cell="FFX" sram_name="LF" local_place="B0W0" owner_tile="OWNER" brick_offset="R0C-1"/>
                    </site_sram>
                  </site>
                </cluster>
              </cluster_info>
            </tile>
          </tile_library>
        </device>
        "##,
    )
    .expect("parse mini cil");

    let device = DeviceDesign {
        name: "mini".to_string(),
        device: "mini".to_string(),
        ports: vec![],
        cells: vec![DeviceCell {
            cell_name: "u_ff".to_string(),
            type_name: "DFFHQ".to_string(),
            properties: Vec::new(),
            site_kind: SiteKind::LogicSlice,
            site_name: "S0".to_string(),
            bel: "FF0".to_string(),
            tile_name: "SRC0".to_string(),
            tile_type: "SOURCE".to_string(),
            x: 0,
            y: 1,
            z: 0,
            cluster_name: Some("clb_0".to_string()),
            synthetic: false,
        }],
        nets: vec![],
        notes: vec![],
    };
    let arch = Arch {
        width: 1,
        height: 2,
        tiles: BTreeMap::from([
            (
                (0, 0),
                TileInstance {
                    name: "OWN0".to_string(),
                    tile_type: "OWNER".to_string(),
                    logic_x: 0,
                    logic_y: 0,
                    bit_x: 0,
                    bit_y: 0,
                    phy_x: 0,
                    phy_y: 0,
                },
            ),
            (
                (0, 1),
                TileInstance {
                    name: "SRC0".to_string(),
                    tile_type: "SOURCE".to_string(),
                    logic_x: 0,
                    logic_y: 1,
                    bit_x: 0,
                    bit_y: 1,
                    phy_x: 0,
                    phy_y: 1,
                },
            ),
        ]),
        ..Arch::default()
    };

    let image = build_config_image(&device, &cil, Some(&arch), None).expect("build config image");
    assert_eq!(image.tiles.len(), 2);

    let source = image
        .tiles
        .iter()
        .find(|tile| tile.tile_name == "SRC0")
        .expect("source tile");
    assert!(
        source
            .configs
            .iter()
            .any(|cfg| cfg.site_name == "S0" && cfg.cfg_name == "FFX")
    );
    assert!(source.assignments.is_empty());

    let owner = image
        .tiles
        .iter()
        .find(|tile| tile.tile_name == "OWN0")
        .expect("owner tile");
    assert_eq!(owner.tile_type, "OWNER");
    assert_eq!(owner.x, 0);
    assert_eq!(owner.y, 0);
    assert!(owner.assignments.iter().any(|bit| bit.site_name == "S0"
        && bit.basic_cell == "FFX"
        && bit.sram_name == "LF"
        && bit.row == 0
        && bit.col == 0
        && bit.value == 1));
    assert!(
        !image
            .notes
            .iter()
            .any(|note| note.contains("not emitted yet"))
    );
}

#[test]
fn explicit_slice_requests_override_default_alias_configs_on_shared_srams() {
    let cil = parse_cil_str(
        r##"
        <device name="mini">
          <site_library>
            <block_site name="SLICE">
              <config_info amount="6">
                <cfg_element name="FFX">
                  <function name="#FF" default="yes">
                    <sram basic_cell="FFX" name="LF" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="INITX">
                  <function name="HIGH" default="yes">
                    <sram basic_cell="FFX" name="INIT" content="1"/>
                    <sram basic_cell="FFX" name="GSR" content="1"/>
                  </function>
                  <function name="LOW" default="no">
                    <sram basic_cell="FFX" name="INIT" content="0"/>
                    <sram basic_cell="FFX" name="GSR" content="0"/>
                  </function>
                </cfg_element>
                <cfg_element name="INITX_BY">
                  <function name="SET" default="yes">
                    <sram basic_cell="FFX" name="GSR" content="1"/>
                  </function>
                  <function name="RESET" default="no">
                    <sram basic_cell="FFX" name="GSR" content="0"/>
                  </function>
                </cfg_element>
                <cfg_element name="SYNC_ATTR">
                  <function name="SYNC" default="yes">
                    <sram basic_cell="FFX" name="AS" content="0"/>
                  </function>
                  <function name="ASYNC" default="no">
                    <sram basic_cell="FFX" name="AS" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="SYNCX">
                  <function name="SYNC" default="yes">
                    <sram basic_cell="FFX" name="AS" content="0"/>
                  </function>
                  <function name="ASYNC" default="no">
                    <sram basic_cell="FFX" name="AS" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="DXMUX">
                  <function name="1" default="yes">
                    <sram basic_cell="DXMUX" name="S0" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="CKINV">
                  <function name="1" default="no">
                    <sram basic_cell="CKINV" name="P" content="1"/>
                  </function>
                </cfg_element>
              </config_info>
            </block_site>
          </site_library>
          <cluster_library>
            <homogeneous_cluster name="SLICE1x1" type="SLICE"/>
          </cluster_library>
          <tile_library>
            <tile name="CENTER" sram_amount="R2C4">
              <cluster_info amount="1">
                <cluster type="SLICE1x1">
                  <site name="S0" position="R0C0">
                    <site_sram>
                      <sram basic_cell="FFX" sram_name="INIT" local_place="B0W0"/>
                      <sram basic_cell="FFX" sram_name="GSR" local_place="B0W1"/>
                      <sram basic_cell="FFX" sram_name="AS" local_place="B0W2"/>
                      <sram basic_cell="FFX" sram_name="LF" local_place="B0W3"/>
                      <sram basic_cell="DXMUX" sram_name="S0" local_place="B1W0"/>
                      <sram basic_cell="CKINV" sram_name="P" local_place="B1W1"/>
                    </site_sram>
                  </site>
                </cluster>
              </cluster_info>
            </tile>
          </tile_library>
        </device>
        "##,
    )
    .expect("parse mini cil");

    let device = DeviceDesign {
        name: "mini".to_string(),
        device: "mini".to_string(),
        ports: vec![],
        cells: vec![DeviceCell {
            cell_name: "u_ff".to_string(),
            type_name: "DFFHQ".to_string(),
            properties: Vec::new(),
            site_kind: SiteKind::LogicSlice,
            site_name: "S0".to_string(),
            bel: "FF0".to_string(),
            tile_name: "T0".to_string(),
            tile_type: "CENTER".to_string(),
            x: 1,
            y: 1,
            z: 0,
            cluster_name: Some("clb_0".to_string()),
            synthetic: false,
        }],
        nets: vec![],
        notes: vec![],
    };

    let image = build_config_image(&device, &cil, None, None).expect("build config image");
    let center = image
        .tiles
        .iter()
        .find(|tile| tile.tile_name == "T0")
        .expect("center tile");

    assert!(
        center
            .assignments
            .iter()
            .any(|bit| bit.basic_cell == "FFX" && bit.sram_name == "INIT" && bit.value == 0)
    );
    assert!(
        center
            .assignments
            .iter()
            .any(|bit| bit.basic_cell == "FFX" && bit.sram_name == "GSR" && bit.value == 0)
    );
    assert!(
        center
            .assignments
            .iter()
            .any(|bit| bit.basic_cell == "FFX" && bit.sram_name == "AS" && bit.value == 1)
    );
}

#[test]
fn lut_only_slice_uses_xused_yused_without_ff_control_bits() {
    let cil = parse_cil_str(
        r##"
        <device name="mini">
          <site_library>
            <block_site name="SLICE">
              <config_info amount="8">
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
                <cfg_element name="FXMUX">
                  <function name="F" default="no">
                    <sram basic_cell="FXMUX" name="S0" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="GYMUX">
                  <function name="G" default="no">
                    <sram basic_cell="GYMUX" name="S0" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="XUSED">
                  <function name="0" default="no">
                    <sram basic_cell="XUSED" name="S0" content="0"/>
                  </function>
                  <function name="1" default="yes">
                    <sram basic_cell="XUSED" name="S0" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="YUSED">
                  <function name="0" default="no">
                    <sram basic_cell="YUSED" name="S0" content="0"/>
                  </function>
                  <function name="1" default="yes">
                    <sram basic_cell="YUSED" name="S0" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="DXMUX">
                  <function name="1" default="yes">
                    <sram basic_cell="DXMUX" name="S0" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="CKINV">
                  <function name="1" default="no">
                    <sram basic_cell="CKINV" name="P" content="1"/>
                  </function>
                </cfg_element>
              </config_info>
            </block_site>
          </site_library>
          <cluster_library>
            <homogeneous_cluster name="SLICE1x1" type="SLICE"/>
          </cluster_library>
          <tile_library>
            <tile name="CENTER" sram_amount="R3C8">
              <cluster_info amount="1">
                <cluster type="SLICE1x1">
                  <site name="S0" position="R0C0">
                    <site_sram>
                      <sram basic_cell="FLUT0" sram_name="SRAM" local_place="B0W0"/>
                      <sram basic_cell="FLUT1" sram_name="SRAM" local_place="B0W1"/>
                      <sram basic_cell="FLUT2" sram_name="SRAM" local_place="B0W2"/>
                      <sram basic_cell="FLUT3" sram_name="SRAM" local_place="B0W3"/>
                      <sram basic_cell="GLUT0" sram_name="SRAM" local_place="B0W4"/>
                      <sram basic_cell="GLUT1" sram_name="SRAM" local_place="B0W5"/>
                      <sram basic_cell="GLUT2" sram_name="SRAM" local_place="B0W6"/>
                      <sram basic_cell="GLUT3" sram_name="SRAM" local_place="B0W7"/>
                      <sram basic_cell="FXMUX" sram_name="S0" local_place="B1W0"/>
                      <sram basic_cell="GYMUX" sram_name="S0" local_place="B1W1"/>
                      <sram basic_cell="XUSED" sram_name="S0" local_place="B1W2"/>
                      <sram basic_cell="YUSED" sram_name="S0" local_place="B1W3"/>
                      <sram basic_cell="DXMUX" sram_name="S0" local_place="B1W4"/>
                      <sram basic_cell="CKINV" sram_name="P" local_place="B1W5"/>
                    </site_sram>
                  </site>
                </cluster>
              </cluster_info>
            </tile>
          </tile_library>
        </device>
        "##,
    )
    .expect("parse mini cil");

    let device = DeviceDesign {
        name: "mini".to_string(),
        device: "mini".to_string(),
        ports: vec![],
        cells: vec![
            DeviceCell {
                cell_name: "u_lut0".to_string(),
                type_name: "LUT2".to_string(),
                properties: vec![Property::new("lut_init", "5")],
                site_kind: SiteKind::LogicSlice,
                site_name: "S0".to_string(),
                bel: "LUT0".to_string(),
                tile_name: "T0".to_string(),
                tile_type: "CENTER".to_string(),
                x: 1,
                y: 1,
                z: 0,
                cluster_name: Some("clb_0".to_string()),
                synthetic: false,
            },
            DeviceCell {
                cell_name: "u_lut1".to_string(),
                type_name: "LUT2".to_string(),
                properties: vec![Property::new("lut_init", "a")],
                site_kind: SiteKind::LogicSlice,
                site_name: "S0".to_string(),
                bel: "LUT1".to_string(),
                tile_name: "T0".to_string(),
                tile_type: "CENTER".to_string(),
                x: 1,
                y: 1,
                z: 0,
                cluster_name: Some("clb_0".to_string()),
                synthetic: false,
            },
        ],
        nets: vec![],
        notes: vec![],
    };

    let image = build_config_image(&device, &cil, None, None).expect("build config image");
    let center = image
        .tiles
        .iter()
        .find(|tile| tile.tile_name == "T0")
        .expect("center tile");

    assert!(
        center
            .configs
            .iter()
            .any(|cfg| cfg.cfg_name == "XUSED" && cfg.function_name == "0")
    );
    assert!(
        center
            .configs
            .iter()
            .any(|cfg| cfg.cfg_name == "YUSED" && cfg.function_name == "0")
    );
    assert!(
        center
            .assignments
            .iter()
            .any(|bit| bit.basic_cell == "XUSED" && bit.value == 0)
    );
    assert!(
        center
            .assignments
            .iter()
            .any(|bit| bit.basic_cell == "YUSED" && bit.value == 0)
    );
    assert!(
        !center
            .assignments
            .iter()
            .any(|bit| matches!(bit.basic_cell.as_str(), "DXMUX" | "CKINV"))
    );
}
