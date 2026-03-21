use super::types::{
    BitstreamCommand, Cil, ClusterDef, ElementDef, ElementPath, MajorFrame, SiteConfigElement,
    SiteDef, SiteFunction, SiteFunctionSram, SramSetting, TileCluster, TileDef, TileSite,
    TileSiteSram, TileTransmission, TransmissionDef,
};
use anyhow::{Context, Result};
use roxmltree::{Document, Node};
use std::{fs, path::Path};

pub fn load_cil(path: &Path) -> Result<Cil> {
    let xml =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    parse_cil_str(&xml).with_context(|| format!("failed to parse {}", path.display()))
}

pub fn parse_cil_str(xml: &str) -> Result<Cil> {
    let doc = Document::parse(xml).context("failed to parse cil xml")?;
    let root = doc.root_element();

    let mut cil = Cil {
        device_name: root.attribute("name").unwrap_or("device").to_string(),
        ..Cil::default()
    };

    parse_element_library(root, &mut cil);
    parse_site_library(root, &mut cil);
    parse_cluster_library(root, &mut cil);
    parse_transmission_library(root, &mut cil);
    parse_tile_library(root, &mut cil);
    parse_major_library(root, &mut cil);
    parse_bitstream_commands(root, &mut cil);

    Ok(cil)
}

fn parse_element_library(root: Node<'_, '_>, cil: &mut Cil) {
    let Some(node) = find_child(root, "element_library") else {
        return;
    };

    for element in node.children().filter(|node| node.has_tag_name("element")) {
        let mut def = ElementDef {
            name: element.attribute("name").unwrap_or_default().to_string(),
            ..ElementDef::default()
        };
        if let Some(srams) = find_child(element, "sram_info") {
            for sram in srams.children().filter(|node| node.has_tag_name("sram")) {
                def.default_srams.push(read_sram_setting(sram));
            }
        }
        if let Some(paths) = find_child(element, "path_info") {
            for path in paths.children().filter(|node| node.has_tag_name("path")) {
                def.paths.push(parse_element_path(path));
            }
        }
        cil.elements.insert(def.name.clone(), def);
    }
}

fn parse_element_path(path: Node<'_, '_>) -> ElementPath {
    let mut entry = ElementPath {
        input: path.attribute("in").unwrap_or_default().to_string(),
        output: path.attribute("out").unwrap_or_default().to_string(),
        segregated: !matches!(path.attribute("segregated"), Some("no") | Some("false")),
        ..ElementPath::default()
    };
    if let Some(config) = find_child(path, "configuration_info") {
        for sram in config.children().filter(|node| node.has_tag_name("sram")) {
            entry.configuration.push(read_sram_setting(sram));
        }
    }
    entry
}

fn parse_site_library(root: Node<'_, '_>, cil: &mut Cil) {
    let Some(node) = find_child(root, "site_library") else {
        return;
    };

    for site in node
        .children()
        .filter(|node| node.has_tag_name("block_site") || node.has_tag_name("primitive_site"))
    {
        let mut def = SiteDef {
            name: site.attribute("name").unwrap_or_default().to_string(),
            ..SiteDef::default()
        };
        if let Some(config) = find_child(site, "config_info") {
            for cfg in config
                .children()
                .filter(|node| node.has_tag_name("cfg_element"))
            {
                def.config_elements.push(parse_config_element(cfg));
            }
        }
        cil.sites.insert(def.name.clone(), def);
    }
}

fn parse_config_element(node: Node<'_, '_>) -> SiteConfigElement {
    let mut cfg_element = SiteConfigElement {
        name: node.attribute("name").unwrap_or_default().to_string(),
        ..SiteConfigElement::default()
    };
    for function in node.children().filter(|node| node.has_tag_name("function")) {
        cfg_element.functions.push(parse_site_function(function));
    }
    cfg_element
}

fn parse_site_function(function: Node<'_, '_>) -> SiteFunction {
    let mut site_fn = SiteFunction {
        name: function.attribute("name").unwrap_or_default().to_string(),
        quomodo: function
            .attribute("quomodo")
            .unwrap_or("naming")
            .to_string(),
        manner: function
            .attribute("manner")
            .unwrap_or("enumeration")
            .to_string(),
        is_default: matches!(function.attribute("default"), Some("yes")),
        ..SiteFunction::default()
    };
    for sram in function.children().filter(|node| node.has_tag_name("sram")) {
        site_fn.srams.push(SiteFunctionSram {
            basic_cell: sram.attribute("basic_cell").unwrap_or_default().to_string(),
            name: sram.attribute("name").unwrap_or_default().to_string(),
            content: parse_u8(sram.attribute("content")).unwrap_or(0),
            address: parse_u8(sram.attribute("address")),
        });
    }
    site_fn
}

fn parse_cluster_library(root: Node<'_, '_>, cil: &mut Cil) {
    let Some(node) = find_child(root, "cluster_library") else {
        return;
    };

    for cluster in node
        .children()
        .filter(|node| node.has_tag_name("homogeneous_cluster"))
    {
        let def = ClusterDef {
            name: cluster.attribute("name").unwrap_or_default().to_string(),
            site_type: cluster.attribute("type").unwrap_or_default().to_string(),
        };
        cil.clusters.insert(def.name.clone(), def);
    }
}

fn parse_transmission_library(root: Node<'_, '_>, cil: &mut Cil) {
    let Some(node) = find_child(root, "transmission_library") else {
        return;
    };

    for transmission in node
        .children()
        .filter(|node| node.has_tag_name("signal_transmission"))
    {
        let def = TransmissionDef {
            name: transmission
                .attribute("name")
                .unwrap_or_default()
                .to_string(),
            site_type: transmission
                .attribute("type")
                .unwrap_or_default()
                .to_string(),
        };
        cil.transmissions.insert(def.name.clone(), def);
    }
}

fn parse_tile_library(root: Node<'_, '_>, cil: &mut Cil) {
    let Some(node) = find_child(root, "tile_library") else {
        return;
    };

    for tile in node.children().filter(|node| node.has_tag_name("tile")) {
        let (sram_rows, sram_cols) =
            parse_rc(tile.attribute("sram_amount").unwrap_or_default()).unwrap_or((0, 0));
        let mut def = TileDef {
            name: tile.attribute("name").unwrap_or_default().to_string(),
            sram_rows,
            sram_cols,
            ..TileDef::default()
        };
        if let Some(cluster_info) = find_child(tile, "cluster_info") {
            for cluster in cluster_info
                .children()
                .filter(|node| node.has_tag_name("cluster"))
            {
                def.clusters.push(parse_tile_cluster(cluster, cil));
            }
        }
        if let Some(transmission_info) = find_child(tile, "transmission_info") {
            for transmission in transmission_info
                .children()
                .filter(|node| node.has_tag_name("transmission"))
            {
                def.transmissions
                    .push(parse_tile_transmission(transmission, cil));
            }
        }
        cil.tiles.insert(def.name.clone(), def);
    }
}

fn parse_tile_cluster(cluster: Node<'_, '_>, cil: &Cil) -> TileCluster {
    let cluster_name = cluster.attribute("type").unwrap_or_default().to_string();
    let site_type = cil
        .clusters
        .get(&cluster_name)
        .map(|entry| entry.site_type.clone())
        .unwrap_or_default();
    let mut tile_cluster = TileCluster {
        cluster_name,
        site_type: site_type.clone(),
        location: cluster.attribute("location").and_then(parse_rc),
        ..TileCluster::default()
    };
    for site in cluster.children().filter(|node| node.has_tag_name("site")) {
        tile_cluster.sites.push(parse_tile_site(site, &site_type));
    }
    tile_cluster
}

fn parse_tile_transmission(transmission: Node<'_, '_>, cil: &Cil) -> TileTransmission {
    let transmission_name = transmission
        .attribute("type")
        .unwrap_or_default()
        .to_string();
    let site_type = cil
        .transmissions
        .get(&transmission_name)
        .map(|entry| entry.site_type.clone())
        .unwrap_or_default();
    let mut tile_transmission = TileTransmission {
        transmission_name,
        site_type: site_type.clone(),
        location: transmission.attribute("location").and_then(parse_rc),
        ..TileTransmission::default()
    };
    for site in transmission
        .children()
        .filter(|node| node.has_tag_name("site"))
    {
        tile_transmission
            .sites
            .push(parse_tile_site(site, &site_type));
    }
    tile_transmission
}

fn parse_tile_site(site: Node<'_, '_>, site_type: &str) -> TileSite {
    let mut tile_site = TileSite {
        name: site.attribute("name").unwrap_or_default().to_string(),
        site_type: site_type.to_string(),
        position: site.attribute("position").and_then(parse_rc),
        ..TileSite::default()
    };
    if let Some(site_sram) = find_child(site, "site_sram") {
        for sram in site_sram
            .children()
            .filter(|node| node.has_tag_name("sram"))
        {
            tile_site.srams.push(parse_tile_site_sram(sram));
        }
    }
    tile_site
}

fn parse_tile_site_sram(sram: Node<'_, '_>) -> TileSiteSram {
    TileSiteSram {
        basic_cell: sram.attribute("basic_cell").unwrap_or_default().to_string(),
        sram_name: sram.attribute("sram_name").unwrap_or_default().to_string(),
        local_place: sram.attribute("local_place").and_then(parse_bw),
        owner_tile: sram.attribute("owner_tile").map(ToString::to_string),
        brick_offset: sram.attribute("brick_offset").and_then(parse_signed_rc),
    }
}

fn parse_major_library(root: Node<'_, '_>, cil: &mut Cil) {
    let Some(node) = find_child(root, "major_library") else {
        return;
    };

    for major in node.children().filter(|node| node.has_tag_name("major")) {
        cil.majors.push(MajorFrame {
            address: parse_usize(major.attribute("address")).unwrap_or(0),
            frame_count: parse_usize(major.attribute("frm_amount")).unwrap_or(0),
            tile_col: parse_usize(major.attribute("tile_col")).unwrap_or(0),
        });
    }
}

fn parse_bitstream_commands(root: Node<'_, '_>, cil: &mut Cil) {
    let Some(node) = find_child(root, "bstrcmd_library") else {
        return;
    };

    for child in node.children().filter(|node| node.is_element()) {
        if child.has_tag_name("parameter") {
            let Some(name) = child.attribute("name") else {
                continue;
            };
            let Some(value) = child.attribute("value") else {
                continue;
            };
            cil.bitstream_parameters
                .insert(name.to_string(), value.to_string());
            continue;
        }
        if child.has_tag_name("command") {
            let Some(cmd) = child.attribute("cmd") else {
                continue;
            };
            cil.bitstream_commands.push(BitstreamCommand {
                cmd: cmd.to_string(),
                parameter: child.attribute("parameter").map(ToString::to_string),
            });
        }
    }
}

fn find_child<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|child| child.has_tag_name(name))
}

fn read_sram_setting(node: Node<'_, '_>) -> SramSetting {
    let defaulted = node.attribute("default").is_some();
    let value = if defaulted {
        parse_u8(node.attribute("default")).unwrap_or(0)
    } else {
        parse_u8(node.attribute("content")).unwrap_or(0)
    };
    SramSetting {
        name: node.attribute("name").unwrap_or_default().to_string(),
        value,
        defaulted,
    }
}

fn parse_rc(raw: &str) -> Option<(usize, usize)> {
    let raw = raw.trim();
    let row = raw.strip_prefix('R')?;
    let (rows, cols) = row.split_once('C')?;
    Some((rows.parse().ok()?, cols.parse().ok()?))
}

fn parse_signed_rc(raw: &str) -> Option<(isize, isize)> {
    let raw = raw.trim();
    let row = raw.strip_prefix('R')?;
    let (rows, cols) = row.split_once('C')?;
    Some((rows.parse().ok()?, cols.parse().ok()?))
}

fn parse_bw(raw: &str) -> Option<(usize, usize)> {
    let raw = raw.trim();
    let bank = raw.strip_prefix('B')?;
    let (rows, cols) = bank.split_once('W')?;
    Some((rows.parse().ok()?, cols.parse().ok()?))
}

fn parse_u8(raw: Option<&str>) -> Option<u8> {
    raw?.trim().parse().ok()
}

fn parse_usize(raw: Option<&str>) -> Option<usize> {
    raw?.trim().parse().ok()
}
