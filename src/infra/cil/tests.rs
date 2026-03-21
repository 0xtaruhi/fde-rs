use super::{load_cil, parse_cil_str};
use anyhow::{Context, Result};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn parses_inline_site_sram_and_function_metadata() -> Result<()> {
    let cil = parse_cil_str(
        r#"
        <device name="mini">
          <site_library>
            <block_site name="SLICE">
              <config_info amount="1">
                <cfg_element name="F">
                  <function name="0x0A" quomodo="srambit" manner="computation" default="no">
                    <sram basic_cell="FLUT0" name="SRAM" address="0"/>
                    <sram basic_cell="FLUT1" name="SRAM" address="1"/>
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
                      <sram basic_cell="FLUT0" sram_name="SRAM" local_place="B1W2"/>
                      <sram basic_cell="FLUT1" sram_name="SRAM" local_place="B1W3" owner_tile="LEFT" brick_offset="R0C1"/>
                    </site_sram>
                  </site>
                </cluster>
              </cluster_info>
            </tile>
          </tile_library>
        </device>
        "#,
    )?;

    let slice = cil.sites.get("SLICE").context("missing SLICE site")?;
    let cfg = slice.config_element("F").context("missing cfg element F")?;
    let function = cfg.function("0x0A").context("missing function 0x0A")?;
    assert_eq!(function.quomodo, "srambit");
    assert_eq!(function.manner, "computation");
    assert_eq!(function.srams.len(), 2);

    let site = cil
        .tile_site("CENTER", "S0")
        .context("missing tile site S0")?;
    assert_eq!(site.srams.len(), 2);
    assert_eq!(site.srams[0].local_place, Some((1, 2)));
    assert_eq!(site.srams[1].owner_tile.as_deref(), Some("LEFT"));
    assert_eq!(site.srams[1].brick_offset, Some((0, 1)));
    assert!(cil.transmissions.is_empty());

    Ok(())
}

#[test]
fn can_parse_external_cil_when_available() -> Result<()> {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return Ok(());
    };
    let path = bundle.root.join("fdp3p7_cil.xml");

    let cil = load_cil(&path)?;
    assert_eq!(cil.device_name, "fdp3000k");
    assert!(cil.elements.len() > 100);
    assert!(cil.sites.contains_key("SLICE"));
    assert!(cil.sites.contains_key("GCLKIOB"));
    assert!(cil.tiles.contains_key("CENTER"));
    assert!(cil.tiles.contains_key("CLKB"));
    assert!(!cil.majors.is_empty());
    assert!(cil.bitstream_parameters.contains_key("FRMLen"));
    assert!(!cil.bitstream_commands.is_empty());
    assert_eq!(
        cil.bitstream_commands
            .first()
            .map(|command| command.cmd.as_str()),
        Some("bsHeader")
    );
    assert!(cil.bitstream_commands.iter().any(|command| {
        command.cmd == "insertCMD"
            && command.parameter.as_deref() == Some("0000_0001, write config")
    }));

    let center = cil.tiles.get("CENTER").context("missing CENTER tile")?;
    assert!(
        center
            .clusters
            .iter()
            .any(|cluster| cluster.site_type == "SLICE" && cluster.sites.len() >= 2)
    );
    assert!(center.transmissions.iter().any(|transmission| {
        transmission.site_type == "GSB_CNT"
            && transmission
                .sites
                .iter()
                .any(|site| site.name == "GSB_CNT" && !site.srams.is_empty())
    }));
    let slice = cil
        .tile_site("CENTER", "S0")
        .context("missing CENTER/S0 site")?;
    assert!(slice.srams.iter().any(|sram| {
        sram.basic_cell == "FLUT0" && sram.sram_name == "SRAM" && sram.local_place.is_some()
    }));
    let gsb = cil
        .tile_transmission_site("CENTER", "GSB_CNT")
        .context("missing CENTER/GSB_CNT transmission site")?;
    assert!(gsb.srams.iter().any(|sram| sram.sram_name == "EN"));
    let gclk = cil.sites.get("GCLK").context("missing GCLK site")?;
    let cemux = gclk
        .config_element("CEMUX")
        .context("missing GCLK/CEMUX config")?;
    assert_eq!(
        cemux
            .default_function()
            .map(|function| function.name.as_str()),
        Some("0")
    );
    assert_eq!(cil.site_name_for_slot("LEFT", "IOB", 1), Some("IOB1"));
    assert_eq!(cil.site_name_for_slot("LEFT", "IOB", 2), Some("IOB2"));
    assert_eq!(cil.site_name_for_slot("LEFT", "IOB", 3), Some("IOB3"));
    assert_eq!(cil.site_name_for_slot("CLKB", "GCLK", 0), Some("GCLKBUF0"));
    assert_eq!(
        cil.site_name_for_slot("CLKB", "GCLKIOB", 0),
        Some("GCLKIOB0")
    );

    Ok(())
}
