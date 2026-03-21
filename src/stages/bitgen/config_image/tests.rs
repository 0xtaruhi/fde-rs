use super::build_config_image;
use crate::{
    cil::parse_cil_str,
    device::{DeviceCell, DeviceDesign, DeviceEndpoint, DeviceNet},
    ir::{PortDirection, Property},
};

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
                <cfg_element name="RAMCONFIG">
                  <function name="2LUTS" default="yes">
                    <sram basic_cell="RAMCFG" name="P" content="1"/>
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
                  <function name="OFF" default="yes"/>
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
                <cfg_element name="OFF">
                  <function name="#FF" default="yes">
                    <sram basic_cell="OFF" name="LF" content="1"/>
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
                      <sram basic_cell="FXMUX" sram_name="S0" local_place="B2W0"/>
                      <sram basic_cell="CKINV" sram_name="P" local_place="B2W1"/>
                      <sram basic_cell="RAMCFG" sram_name="P" local_place="B2W2"/>
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
                      <sram basic_cell="OFF" sram_name="LF" local_place="B0W3"/>
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
                properties: vec![Property {
                    key: "lut_init".to_string(),
                    value: "10".to_string(),
                }],
                site_kind: "SLICE".to_string(),
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
                site_kind: "SLICE".to_string(),
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
                site_kind: "IOB".to_string(),
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
                site_kind: "GCLK".to_string(),
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
                kind: "cell".to_string(),
                name: "u_ff".to_string(),
                pin: "Q".to_string(),
                x: 1,
                y: 1,
                z: 0,
            }),
            sinks: vec![DeviceEndpoint {
                kind: "cell".to_string(),
                name: "$iob$out".to_string(),
                pin: "OUT".to_string(),
                x: 3,
                y: 0,
                z: 0,
            }],
            origin: "logical-net".to_string(),
            route_pips: Vec::new(),
            guide_tiles: Vec::new(),
            sink_guides: Vec::new(),
        }],
        notes: vec![PortDirection::Output.as_str().to_string()],
    };

    let image = build_config_image(&device, &cil, None).expect("build config image");
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
            .any(|cfg| cfg.cfg_name == "F" && cfg.function_name == "10")
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
    assert!(center.configs.iter().all(|cfg| cfg.cfg_name != "RAMCONFIG"));
    assert!(
        center
            .assignments
            .iter()
            .all(|bit| bit.basic_cell != "RAMCFG")
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
            .configs
            .iter()
            .any(|cfg| cfg.cfg_name == "OUTMUX" && cfg.function_name == "1")
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
    assert!(right.configs.iter().all(|cfg| cfg.cfg_name != "OFF"));
    assert!(right.assignments.iter().all(|bit| bit.basic_cell != "OFF"));

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
fn expands_small_lut_truth_tables_to_site_width() {
    let cil = parse_cil_str(
        r##"
        <device name="mini">
          <site_library>
            <block_site name="SLICE">
              <config_info amount="2">
                <cfg_element name="F">
                  <function name="1" quomodo="equation" manner="computation" default="yes">
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
                <cfg_element name="FXMUX">
                  <function name="F" default="no">
                    <sram basic_cell="FXMUX" name="S0" content="1"/>
                  </function>
                </cfg_element>
              </config_info>
            </block_site>
          </site_library>
          <cluster_library>
            <homogeneous_cluster name="SLICE1x1" type="SLICE"/>
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
                      <sram basic_cell="FLUT4" sram_name="SRAM" local_place="B1W0"/>
                      <sram basic_cell="FLUT5" sram_name="SRAM" local_place="B1W1"/>
                      <sram basic_cell="FLUT6" sram_name="SRAM" local_place="B1W2"/>
                      <sram basic_cell="FLUT7" sram_name="SRAM" local_place="B1W3"/>
                      <sram basic_cell="FLUT8" sram_name="SRAM" local_place="B2W0"/>
                      <sram basic_cell="FLUT9" sram_name="SRAM" local_place="B2W1"/>
                      <sram basic_cell="FLUT10" sram_name="SRAM" local_place="B2W2"/>
                      <sram basic_cell="FLUT11" sram_name="SRAM" local_place="B2W3"/>
                      <sram basic_cell="FLUT12" sram_name="SRAM" local_place="B3W0"/>
                      <sram basic_cell="FLUT13" sram_name="SRAM" local_place="B3W1"/>
                      <sram basic_cell="FLUT14" sram_name="SRAM" local_place="B3W2"/>
                      <sram basic_cell="FLUT15" sram_name="SRAM" local_place="B3W3"/>
                      <sram basic_cell="FXMUX" sram_name="S0" local_place="B3W4"/>
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
            properties: vec![Property {
                key: "lut_init".to_string(),
                value: "10".to_string(),
            }],
            site_kind: "SLICE".to_string(),
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

    let image = build_config_image(&device, &cil, None).expect("build config image");
    let center = image
        .tiles
        .iter()
        .find(|tile| tile.tile_name == "T0")
        .expect("center tile");

    assert!(
        center
            .configs
            .iter()
            .any(|cfg| cfg.cfg_name == "F" && cfg.function_name == "#LUT:D=((A3*~A2)*~A1)")
    );
    for basic_cell in ["FLUT4", "FLUT12"] {
        assert!(
            center
                .assignments
                .iter()
                .any(|bit| bit.basic_cell == basic_cell && bit.value == 1)
        );
    }
    for basic_cell in [
        "FLUT0", "FLUT1", "FLUT2", "FLUT3", "FLUT5", "FLUT6", "FLUT7", "FLUT8", "FLUT9", "FLUT10",
        "FLUT11", "FLUT13", "FLUT14", "FLUT15",
    ] {
        assert!(
            center
                .assignments
                .iter()
                .all(|bit| bit.basic_cell != basic_cell || bit.value == 0)
        );
    }
}
