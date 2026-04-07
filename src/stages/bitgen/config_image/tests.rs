use super::encode_config_image;
use crate::{
    bitgen::{ProgrammedSite, ProgrammingImage, RequestedConfig},
    cil::parse_cil_str,
    domain::SiteKind,
    resource::{Arch, TileInstance},
};
use std::collections::BTreeMap;

fn request(cfg_name: &str, function_name: &str) -> RequestedConfig {
    RequestedConfig::new(cfg_name, function_name)
}

fn programmed_site(
    tile_name: &str,
    tile_type: &str,
    site_kind: SiteKind,
    site_name: &str,
    x: usize,
    y: usize,
    requests: &[(&str, &str)],
) -> ProgrammedSite {
    ProgrammedSite::new(
        tile_name,
        tile_type,
        site_kind,
        site_name,
        x,
        y,
        requests
            .iter()
            .map(|(cfg_name, function_name)| request(cfg_name, function_name))
            .collect(),
    )
}

#[test]
fn builds_tile_assignments_for_slice_iob_and_gclk_programming_requests() {
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
                <cfg_element name="CKINV">
                  <function name="1" default="no">
                    <sram basic_cell="CKINV" name="P" content="1"/>
                  </function>
                </cfg_element>
              </config_info>
            </block_site>
            <block_site name="IOB">
              <config_info amount="3">
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

    let programming = ProgrammingImage {
        sites: vec![
            programmed_site(
                "T0",
                "CENTER",
                SiteKind::LogicSlice,
                "S0",
                1,
                1,
                &[
                    ("F", "0xA"),
                    ("FFX", "#FF"),
                    ("INITX", "LOW"),
                    ("SYNCX", "ASYNC"),
                    ("DXMUX", "1"),
                    ("CKINV", "1"),
                ],
            ),
            programmed_site(
                "IO0",
                "RIGHT",
                SiteKind::Iob,
                "IOB0",
                3,
                0,
                &[("OMUX", "O"), ("OUTMUX", "1"), ("DRIVEATTRBOX", "12")],
            ),
            programmed_site(
                "CLK0",
                "CLKB",
                SiteKind::Gclk,
                "GCLKBUF0",
                4,
                0,
                &[("CEMUX", "1"), ("DISABLE_ATTR", "LOW")],
            ),
        ],
        ..ProgrammingImage::default()
    };

    let image = encode_config_image(&programming, &cil, None).expect("encode config image");
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
        center
            .assignments
            .iter()
            .any(|bit| bit.basic_cell == "DXMUX" && bit.value == 1)
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
fn encodes_addressed_lut_requests_into_site_srams() {
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

    let programming = ProgrammingImage {
        sites: vec![programmed_site(
            "T0",
            "CENTER",
            SiteKind::LogicSlice,
            "S0",
            1,
            1,
            &[("F", "0x1111")],
        )],
        ..ProgrammingImage::default()
    };

    let image = encode_config_image(&programming, &cil, None).expect("encode config image");
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

    let programming = ProgrammingImage {
        sites: vec![programmed_site(
            "SRC0",
            "SOURCE",
            SiteKind::LogicSlice,
            "S0",
            0,
            1,
            &[("FFX", "#FF")],
        )],
        ..ProgrammingImage::default()
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

    let image = encode_config_image(&programming, &cil, Some(&arch)).expect("encode config image");
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
fn block_ram_site_requests_encode_into_owner_tiles() {
    let cil = parse_cil_str(
        r##"
        <device name="mini">
          <site_library>
            <block_site name="BRAM">
              <config_info amount="2">
                <cfg_element name="PORTA_ATTR">
                  <function name="2048X2" default="no">
                    <sram basic_cell="BLOCKRAM" name="PORTA_ATTR0" content="1"/>
                  </function>
                </cfg_element>
                <cfg_element name="ENAMUX">
                  <function name="ENA" default="no">
                    <sram basic_cell="BLOCKRAM" name="ENAMUX0" content="1"/>
                  </function>
                </cfg_element>
              </config_info>
            </block_site>
          </site_library>
          <cluster_library>
            <homogeneous_cluster name="BRAM1x1" type="BRAM"/>
          </cluster_library>
          <tile_library>
            <tile name="OWNER" sram_amount="R1C2"/>
            <tile name="SOURCE" sram_amount="R1C2">
              <cluster_info amount="1">
                <cluster type="BRAM1x1">
                  <site name="BRAM" position="R0C0">
                    <site_sram>
                      <sram basic_cell="BLOCKRAM" sram_name="PORTA_ATTR0" local_place="B0W0" owner_tile="OWNER" brick_offset="R-2C0"/>
                      <sram basic_cell="BLOCKRAM" sram_name="ENAMUX0" local_place="B0W1" owner_tile="OWNER" brick_offset="R-2C0"/>
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

    let programming = ProgrammingImage {
        sites: vec![programmed_site(
            "SRC0",
            "SOURCE",
            SiteKind::BlockRam,
            "BRAM",
            2,
            0,
            &[("PORTA_ATTR", "2048X2"), ("ENAMUX", "ENA")],
        )],
        ..ProgrammingImage::default()
    };
    let arch = Arch {
        width: 3,
        height: 1,
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
                (2, 0),
                TileInstance {
                    name: "SRC0".to_string(),
                    tile_type: "SOURCE".to_string(),
                    logic_x: 2,
                    logic_y: 0,
                    bit_x: 2,
                    bit_y: 0,
                    phy_x: 2,
                    phy_y: 0,
                },
            ),
        ]),
        ..Arch::default()
    };

    let image = encode_config_image(&programming, &cil, Some(&arch)).expect("encode config image");

    let source = image
        .tiles
        .iter()
        .find(|tile| tile.tile_name == "SRC0")
        .expect("source tile");
    assert!(
        source
            .configs
            .iter()
            .any(|cfg| cfg.site_name == "BRAM" && cfg.cfg_name == "PORTA_ATTR")
    );
    assert!(source.assignments.is_empty());

    let owner = image
        .tiles
        .iter()
        .find(|tile| tile.tile_name == "OWN0")
        .expect("owner tile");
    assert_eq!(owner.tile_type, "OWNER");
    assert!(owner.assignments.iter().any(|bit| {
        bit.site_name == "BRAM"
            && bit.cfg_name == "PORTA_ATTR"
            && bit.basic_cell == "BLOCKRAM"
            && bit.sram_name == "PORTA_ATTR0"
            && bit.row == 0
            && bit.col == 0
            && bit.value == 1
    }));
    assert!(owner.assignments.iter().any(|bit| {
        bit.site_name == "BRAM"
            && bit.cfg_name == "ENAMUX"
            && bit.basic_cell == "BLOCKRAM"
            && bit.sram_name == "ENAMUX0"
            && bit.row == 0
            && bit.col == 1
            && bit.value == 1
    }));
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

    let programming = ProgrammingImage {
        sites: vec![programmed_site(
            "T0",
            "CENTER",
            SiteKind::LogicSlice,
            "S0",
            1,
            1,
            &[
                ("FFX", "#FF"),
                ("INITX", "LOW"),
                ("SYNC_ATTR", "ASYNC"),
                ("SYNCX", "ASYNC"),
                ("DXMUX", "1"),
                ("CKINV", "1"),
            ],
        )],
        ..ProgrammingImage::default()
    };

    let image = encode_config_image(&programming, &cil, None).expect("encode config image");
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
