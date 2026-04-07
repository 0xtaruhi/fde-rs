use super::{BitgenOptions, DeviceCell, DeviceDesign, run};
use crate::{
    cil::parse_cil_str,
    domain::{CellKind, SiteKind},
    ir::{BitstreamImage, Cluster, Design, Net, RouteSegment},
};
use anyhow::Result;
use std::fs;
use tempfile::NamedTempFile;

fn write_temp_arch_file() -> NamedTempFile {
    let file = NamedTempFile::new().expect("temp arch file");
    fs::write(
        file.path(),
        r#"
        <device name="mini">
          <device_info scale="1,1" slice_per_tile="2" LUT_Inputs="4"/>
          <library name="arch">
            <module name="mini">
              <instance
                name="BRAMR4C0"
                cellRef="LBRAMD"
                libraryRef="tile"
                logic_pos="0,0"
                bit_pos="0,0"
                phy_pos="0,0"/>
            </module>
          </library>
        </device>
        "#,
    )
    .expect("write arch xml");
    file
}

fn mini_bram_cil() -> crate::cil::Cil {
    parse_cil_str(
        r##"
        <device name="mini">
          <site_library>
            <block_site name="BRAM">
              <config_info amount="1">
                <cfg_element name="PORTA_ATTR">
                  <function name="4096X1" quomodo="naming" manner="computation" default="no">
                    <sram basic_cell="BLOCKRAM" name="PORTA_ATTR0" content="1"/>
                  </function>
                </cfg_element>
              </config_info>
            </block_site>
          </site_library>
          <tile_library>
            <tile name="LBRAMD" sram_amount="R1C1">
              <cluster_info amount="1">
                <homogeneous_cluster name="BRAM1x1" type="BRAM"/>
                <cluster type="BRAM1x1">
                  <site name="BRAM" position="R0C0">
                    <site_sram amount="1">
                      <sram basic_cell="BLOCKRAM" sram_name="PORTA_ATTR0" local_place="B0W0"/>
                    </site_sram>
                  </site>
                </cluster>
              </cluster_info>
            </tile>
          </tile_library>
          <major_library>
            <major address="0" frm_amount="1" tile_col="0"/>
          </major_library>
          <bstrcmd_library>
            <parameter name="bits_per_grp_reversed" value="1"/>
            <parameter name="initialNum" value="0"/>
            <parameter name="FRMLen" value="1"/>
            <parameter name="major_shift" value="17"/>
            <parameter name="mem_amount" value="1"/>
            <parameter name="wrdsAmnt_shift" value="0"/>
            <parameter name="fillblank" value="0"/>
            <command cmd="bsHeader"/>
            <command cmd="adjustSYNC"/>
            <command cmd="setFRMLen"/>
            <command cmd="writeNomalTiles"/>
            <command cmd="writeMem"/>
          </bstrcmd_library>
        </device>
        "##,
    )
    .expect("parse mini bram cil")
}

fn mini_bram_device_design() -> DeviceDesign {
    DeviceDesign {
        cells: vec![
            DeviceCell::new("ram0", CellKind::BlockRam, "BLOCKRAM_1")
                .with_properties(vec![
                    crate::ir::Property::new("port_attr", "4096X1"),
                    crate::ir::Property::new(
                        "init_00",
                        "256'h0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    ),
                    crate::ir::Property::new(
                        "init_01",
                        "256'hfedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
                    ),
                ])
                .placed(
                    SiteKind::BlockRam,
                    "BRAM",
                    "BRAM",
                    "BRAMR4C0",
                    "LBRAMD",
                    (0, 0, 0),
                ),
        ],
        ..DeviceDesign::default()
    }
}

fn routed_design() -> Design {
    Design {
        name: "bitgen-mini".to_string(),
        stage: "timed".to_string(),
        clusters: vec![
            Cluster::logic("clb0")
                .with_member("u0")
                .with_capacity(1)
                .at(1, 1),
        ],
        nets: vec![
            Net::new("mid")
                .with_route_segment(RouteSegment::new((0, 1), (1, 1)))
                .with_route_segment(RouteSegment::new((1, 1), (2, 1))),
        ],
        ..Design::default()
    }
}

fn assert_image(image: &BitstreamImage) {
    assert!(image.bytes.starts_with(b"FDEBIT24"));
    assert_eq!(image.design_name, "bitgen-mini");
    assert_eq!(image.sha256.len(), 64);
    assert!(image.sidecar_text.contains("mode=deterministic-payload"));
    assert!(image.sidecar_text.contains("CLUSTER clb0"));
    assert!(image.sidecar_text.contains("NET mid len=2"));
}

#[test]
fn falls_back_to_deterministic_payload_without_resources() -> Result<()> {
    let result = run(routed_design(), &BitgenOptions::default())?;
    assert_image(&result.value);
    assert!(
        result
            .report
            .messages
            .iter()
            .any(|message| message.contains("deterministic bitstream payload"))
    );
    Ok(())
}

#[test]
fn architecture_backed_bitgen_emits_bram_memory_payloads_from_lowercase_init_properties()
-> Result<()> {
    let arch_file = write_temp_arch_file();
    let result = run(
        Design {
            name: "bram-memory-mini".to_string(),
            stage: "placed".to_string(),
            ..Design::default()
        },
        &BitgenOptions {
            arch_name: Some("mini".to_string()),
            arch_path: Some(arch_file.path().to_path_buf()),
            cil: Some(mini_bram_cil()),
            device_design: Some(mini_bram_device_design()),
            ..BitgenOptions::default()
        },
    )?;

    let text = String::from_utf8(result.value.bytes).expect("text bitstream");
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| *line == "0202_0000")
        .expect("first memory block address");
    assert_eq!(lines[start + 3], "89ab_cdef");
    assert_eq!(lines[start + 4], "0123_4567");
    assert_eq!(lines[start + 11], "7654_3210");
    assert_eq!(lines[start + 12], "fedc_ba98");
    assert!(
        result.report.messages.iter().any(|message| {
            message.contains("Generated") && message.contains("1 memory chunks")
        })
    );

    Ok(())
}
