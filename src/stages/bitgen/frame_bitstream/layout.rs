use super::super::config_image::{
    ConfigResolution, find_route_sram, find_tile_sram, resolve_site_config,
};
use crate::{
    cil::{Cil, TileSiteSram},
    config_image::{ConfigImage, TileBitAssignment, TileConfigImage},
    domain::SiteKind,
    resource::{Arch, TileInstance},
    route_bits::RouteBit,
};
use std::collections::HashMap;

use super::model::{DEFAULT_FILL_BIT, TileColumns, TileFrameImage};

pub(crate) fn build_tile_columns(
    arch: &Arch,
    cil: &Cil,
    config_image: &ConfigImage,
    transmission_defaults: &HashMap<String, Vec<RouteBit>>,
    notes: &mut Vec<String>,
) -> TileColumns {
    let mut tiles_by_name = build_arch_tile_images(arch, cil, transmission_defaults, notes);
    apply_config_assignments(arch, cil, &mut tiles_by_name, config_image, notes);

    let mut columns = TileColumns::new();
    for image in tiles_by_name.into_values() {
        columns.entry(image.bit_y).or_default().push(image);
    }
    for tiles in columns.values_mut() {
        tiles.sort_by(|lhs, rhs| {
            (lhs.bit_x, lhs.tile_name.as_str()).cmp(&(rhs.bit_x, rhs.tile_name.as_str()))
        });
    }
    columns
}

fn build_arch_tile_images(
    arch: &Arch,
    cil: &Cil,
    transmission_defaults: &HashMap<String, Vec<RouteBit>>,
    notes: &mut Vec<String>,
) -> std::collections::BTreeMap<String, TileFrameImage> {
    let mut tiles_by_name = std::collections::BTreeMap::<String, TileFrameImage>::new();
    for tile in arch.tiles.values() {
        let Some(tile_def) = cil.tiles.get(&tile.tile_type) else {
            notes.push(format!(
                "Missing CIL tile definition for architecture tile {} ({}).",
                tile.name, tile.tile_type
            ));
            continue;
        };
        tiles_by_name.insert(
            tile.name.clone(),
            TileFrameImage {
                tile_name: tile.name.clone(),
                tile_type: tile.tile_type.clone(),
                bit_x: tile.bit_x,
                bit_y: tile.bit_y,
                rows: tile_def.sram_rows,
                cols: tile_def.sram_cols,
                bits: vec![DEFAULT_FILL_BIT; tile_def.sram_rows.saturating_mul(tile_def.sram_cols)],
            },
        );
    }
    for tile in arch.tiles.values() {
        let Some(tile_def) = cil.tiles.get(&tile.tile_type) else {
            continue;
        };
        apply_tile_site_defaults(arch, &mut tiles_by_name, tile, tile_def, cil, notes);
        apply_tile_transmission_defaults(
            arch,
            &mut tiles_by_name,
            tile,
            tile_def,
            transmission_defaults,
            notes,
        );
    }
    tiles_by_name
}

fn apply_tile_site_defaults(
    arch: &Arch,
    tiles_by_name: &mut std::collections::BTreeMap<String, TileFrameImage>,
    source_tile: &TileInstance,
    tile_def: &crate::cil::TileDef,
    cil: &Cil,
    notes: &mut Vec<String>,
) {
    for tile_site in tile_def
        .clusters
        .iter()
        .flat_map(|cluster| cluster.sites.iter())
    {
        if tile_site.srams.is_empty() {
            continue;
        }
        let site_kind = SiteKind::classify(&tile_site.site_type);
        let Some(site_def) = cil
            .sites
            .get(&tile_site.site_type)
            .or_else(|| cil.site_def(site_kind))
        else {
            continue;
        };
        for cfg in &site_def.config_elements {
            let Some(default_function) = cfg.default_function() else {
                continue;
            };
            if default_function.srams.is_empty() {
                continue;
            }
            let ConfigResolution::Matched(bits) =
                resolve_site_config(site_def, &cfg.name, &default_function.name)
            else {
                notes.push(format!(
                    "Could not resolve default config {}={} for {}:{}.",
                    cfg.name, default_function.name, tile_def.name, tile_site.name
                ));
                continue;
            };
            for bit in bits {
                let Some(mapping) = find_tile_sram(tile_site, &bit) else {
                    notes.push(format!(
                        "Missing default site SRAM mapping for {}:{}:{} on {}:{}.",
                        bit.cfg_name, bit.basic_cell, bit.sram_name, tile_def.name, tile_site.name
                    ));
                    continue;
                };
                let context = format!(
                    "default site bit {}:{}:{} on {}:{}",
                    bit.cfg_name, bit.basic_cell, bit.sram_name, tile_def.name, tile_site.name
                );
                apply_mapped_bit(
                    arch,
                    tiles_by_name,
                    source_tile,
                    mapping,
                    bit.value,
                    &context,
                    notes,
                );
            }
        }
    }
}

fn apply_tile_transmission_defaults(
    arch: &Arch,
    tiles_by_name: &mut std::collections::BTreeMap<String, TileFrameImage>,
    source_tile: &TileInstance,
    tile_def: &crate::cil::TileDef,
    transmission_defaults: &HashMap<String, Vec<RouteBit>>,
    notes: &mut Vec<String>,
) {
    for transmission_site in tile_def
        .transmissions
        .iter()
        .flat_map(|transmission| transmission.sites.iter())
    {
        if transmission_site.srams.is_empty() {
            continue;
        }
        let Some(default_bits) = transmission_defaults.get(&transmission_site.site_type) else {
            continue;
        };
        for bit in default_bits {
            let Some(mapping) = find_route_sram(transmission_site, bit) else {
                notes.push(format!(
                    "Missing default transmission SRAM mapping for {}:{} on {}:{}.",
                    bit.basic_cell, bit.sram_name, tile_def.name, transmission_site.name
                ));
                continue;
            };
            let context = format!(
                "default transmission bit {}:{} on {}:{}",
                bit.basic_cell, bit.sram_name, tile_def.name, transmission_site.name
            );
            apply_mapped_bit(
                arch,
                tiles_by_name,
                source_tile,
                mapping,
                bit.value,
                &context,
                notes,
            );
        }
    }
}

fn apply_config_assignments(
    arch: &Arch,
    cil: &Cil,
    tiles_by_name: &mut std::collections::BTreeMap<String, TileFrameImage>,
    config_image: &ConfigImage,
    notes: &mut Vec<String>,
) {
    for tile in &config_image.tiles {
        let Some(source_image) = tiles_by_name.get(&tile.tile_name) else {
            notes.push(format!(
                "Config image tile {} does not exist in the architecture layout.",
                tile.tile_name
            ));
            continue;
        };

        if source_image.tile_type != tile.tile_type {
            notes.push(format!(
                "Config image tile {} changed type from {} to {}.",
                tile.tile_name, source_image.tile_type, tile.tile_type
            ));
        }
        if source_image.rows != tile.rows || source_image.cols != tile.cols {
            notes.push(format!(
                "Config image tile {} shape {}x{} does not match CIL shape {}x{}.",
                tile.tile_name, tile.rows, tile.cols, source_image.rows, source_image.cols
            ));
        }

        let Some(source_tile) = source_tile_instance(arch, tile) else {
            notes.push(format!(
                "Config image tile {} at {},{} does not resolve to an architecture tile.",
                tile.tile_name, tile.x, tile.y
            ));
            continue;
        };

        for assignment in &tile.assignments {
            if let Some(mapping) = assignment_mapping(cil, &tile.tile_type, assignment) {
                let context = format!(
                    "config bit {}:{}:{} on {}:{}",
                    assignment.cfg_name,
                    assignment.basic_cell,
                    assignment.sram_name,
                    tile.tile_type,
                    assignment.site_name
                );
                apply_mapped_bit(
                    arch,
                    tiles_by_name,
                    source_tile,
                    mapping,
                    assignment.value,
                    &context,
                    notes,
                );
                continue;
            }

            write_tile_bit(
                tiles_by_name,
                &tile.tile_name,
                assignment.row,
                assignment.col,
                assignment.value,
                &format!(
                    "config bit {}:{}:{} on {}:{}",
                    assignment.cfg_name,
                    assignment.basic_cell,
                    assignment.sram_name,
                    tile.tile_type,
                    assignment.site_name
                ),
                notes,
            );
        }
    }
}

fn source_tile_instance<'a>(arch: &'a Arch, tile: &TileConfigImage) -> Option<&'a TileInstance> {
    arch.tile_at(tile.x, tile.y)
        .filter(|source_tile| source_tile.name == tile.tile_name)
        .or_else(|| {
            arch.tiles
                .values()
                .find(|source_tile| source_tile.name == tile.tile_name)
        })
}

fn assignment_mapping<'a>(
    cil: &'a Cil,
    tile_type: &str,
    assignment: &TileBitAssignment,
) -> Option<&'a TileSiteSram> {
    let tile_def = cil.tiles.get(tile_type)?;
    tile_def
        .clusters
        .iter()
        .flat_map(|cluster| cluster.sites.iter())
        .chain(
            tile_def
                .transmissions
                .iter()
                .flat_map(|transmission| transmission.sites.iter()),
        )
        .find(|site| site.name == assignment.site_name)
        .and_then(|site| {
            site.srams
                .iter()
                .find(|sram| {
                    sram.basic_cell == assignment.basic_cell
                        && sram.sram_name == assignment.sram_name
                })
                .or_else(|| {
                    site.srams.iter().find(|sram| {
                        sram.basic_cell.is_empty()
                            && assignment.basic_cell.is_empty()
                            && sram.sram_name == assignment.sram_name
                    })
                })
        })
}

fn apply_mapped_bit(
    arch: &Arch,
    tiles_by_name: &mut std::collections::BTreeMap<String, TileFrameImage>,
    source_tile: &TileInstance,
    mapping: &TileSiteSram,
    value: u8,
    context: &str,
    notes: &mut Vec<String>,
) {
    let Some((target_tile_name, row, col)) =
        mapped_tile_target(arch, source_tile, mapping, context, notes)
    else {
        return;
    };
    write_tile_bit(
        tiles_by_name,
        target_tile_name,
        row,
        col,
        value,
        context,
        notes,
    );
}

fn mapped_tile_target<'a>(
    arch: &'a Arch,
    source_tile: &'a TileInstance,
    mapping: &TileSiteSram,
    context: &str,
    notes: &mut Vec<String>,
) -> Option<(&'a str, usize, usize)> {
    let Some((row, col)) = mapping.local_place else {
        notes.push(format!("Local place is missing for {context}."));
        return None;
    };

    let target_tile = relocate_tile_instance(arch, source_tile, mapping, context, notes)?;

    if let Some(owner_tile) = mapping.owner_tile.as_deref()
        && target_tile.tile_type != owner_tile
    {
        notes.push(format!(
            "Owner-tile remap for {context} expected tile type {owner_tile}, found {}.",
            target_tile.tile_type
        ));
    }

    Some((target_tile.name.as_str(), row, col))
}

fn relocate_tile_instance<'a>(
    arch: &'a Arch,
    source_tile: &'a TileInstance,
    mapping: &TileSiteSram,
    context: &str,
    notes: &mut Vec<String>,
) -> Option<&'a TileInstance> {
    let Some((row_offset, col_offset)) = mapping.brick_offset else {
        return Some(source_tile);
    };

    let Some(target_x) = source_tile.logic_x.checked_add_signed(row_offset) else {
        notes.push(format!(
            "Owner-tile remap for {context} overflows row from {} by {}.",
            source_tile.logic_x, row_offset
        ));
        return None;
    };
    let Some(target_y) = source_tile.logic_y.checked_add_signed(col_offset) else {
        notes.push(format!(
            "Owner-tile remap for {context} overflows column from {} by {}.",
            source_tile.logic_y, col_offset
        ));
        return None;
    };

    let Some(target_tile) = arch.tile_at(target_x, target_y) else {
        notes.push(format!(
            "Owner-tile remap for {context} targets missing tile at {target_x},{target_y}."
        ));
        return None;
    };

    Some(target_tile)
}

fn write_tile_bit(
    tiles_by_name: &mut std::collections::BTreeMap<String, TileFrameImage>,
    tile_name: &str,
    row: usize,
    col: usize,
    value: u8,
    context: &str,
    notes: &mut Vec<String>,
) {
    let Some(image) = tiles_by_name.get_mut(tile_name) else {
        notes.push(format!(
            "Bit target tile {tile_name} for {context} does not exist in the frame layout."
        ));
        return;
    };
    if row >= image.rows || col >= image.cols {
        notes.push(format!(
            "Bit target {row}:{col} for {context} falls outside tile {} bounds {}x{}.",
            image.tile_name, image.rows, image.cols
        ));
        return;
    }
    let offset = row * image.cols + col;
    image.bits[offset] = value & 1;
}

#[cfg(test)]
mod tests {
    use super::build_tile_columns;
    use crate::{
        cil::parse_cil_str,
        config_image::{ConfigImage, TileBitAssignment, TileConfigImage},
        resource::{Arch, TileInstance},
        route_bits::RouteBit,
    };
    use std::collections::{BTreeMap, HashMap};

    #[test]
    fn applies_default_transmission_bits_into_frame_images() {
        let cil = parse_cil_str(
            r##"
            <device name="mini">
              <transmission_library>
                <signal_transmission name="CNTX" type="GSB_CNT"/>
              </transmission_library>
              <tile_library>
                <tile name="CENTER" sram_amount="R1C1">
                  <transmission_info amount="1">
                    <transmission type="CNTX">
                      <site name="GSB_CNT" position="R0C0">
                        <site_sram>
                          <sram basic_cell="sw0" sram_name="EN" local_place="B0W0"/>
                        </site_sram>
                      </site>
                    </transmission>
                  </transmission_info>
                </tile>
              </tile_library>
            </device>
            "##,
        )
        .expect("parse mini cil");

        let arch = Arch {
            width: 1,
            height: 1,
            tiles: BTreeMap::from([(
                (0, 0),
                TileInstance {
                    name: "T0".to_string(),
                    tile_type: "CENTER".to_string(),
                    logic_x: 0,
                    logic_y: 0,
                    bit_x: 0,
                    bit_y: 0,
                    phy_x: 0,
                    phy_y: 0,
                },
            )]),
            ..Arch::default()
        };
        let transmission_defaults = HashMap::from([(
            "GSB_CNT".to_string(),
            vec![RouteBit {
                basic_cell: "sw0".to_string(),
                sram_name: "EN".to_string(),
                value: 0,
            }],
        )]);

        let mut notes = Vec::new();
        let columns = build_tile_columns(
            &arch,
            &cil,
            &ConfigImage::default(),
            &transmission_defaults,
            &mut notes,
        );
        let tile = &columns
            .get(&0)
            .expect("column 0")
            .first()
            .expect("tile image");

        assert_eq!(tile.bits, vec![0]);
        assert!(notes.is_empty());
    }

    #[test]
    fn relocates_default_site_bits_into_owner_tiles() {
        let cil = parse_cil_str(
            r##"
            <device name="mini">
              <site_library>
                <block_site name="SRC">
                  <config_info amount="1">
                    <cfg_element name="MODE">
                      <function name="ON" default="yes">
                        <sram basic_cell="CFG" name="BIT" content="0"/>
                      </function>
                    </cfg_element>
                  </config_info>
                </block_site>
              </site_library>
              <cluster_library>
                <homogeneous_cluster name="SRC1x1" type="SRC"/>
              </cluster_library>
              <tile_library>
                <tile name="OWNER" sram_amount="R1C1"/>
                <tile name="SOURCE" sram_amount="R1C1">
                  <cluster_info amount="1">
                    <cluster type="SRC1x1">
                      <site name="SRC0" position="R0C0">
                        <site_sram>
                          <sram basic_cell="CFG" sram_name="BIT" local_place="B0W0" owner_tile="OWNER" brick_offset="R0C-1"/>
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

        let mut notes = Vec::new();
        let columns = build_tile_columns(
            &arch,
            &cil,
            &ConfigImage::default(),
            &HashMap::new(),
            &mut notes,
        );

        let owner = columns
            .get(&0)
            .expect("owner column")
            .first()
            .expect("owner tile");
        let source = columns
            .get(&1)
            .expect("source column")
            .first()
            .expect("source tile");

        assert_eq!(owner.bits, vec![0]);
        assert_eq!(source.bits, vec![1]);
        assert!(notes.is_empty());
    }

    #[test]
    fn relocates_config_assignments_into_owner_tiles() {
        let cil = parse_cil_str(
            r##"
            <device name="mini">
              <site_library>
                <block_site name="SRC">
                  <config_info amount="1">
                    <cfg_element name="MODE">
                      <function name="ON">
                        <sram basic_cell="CFG" name="BIT" content="0"/>
                      </function>
                    </cfg_element>
                  </config_info>
                </block_site>
              </site_library>
              <cluster_library>
                <homogeneous_cluster name="SRC1x1" type="SRC"/>
              </cluster_library>
              <tile_library>
                <tile name="OWNER" sram_amount="R1C1"/>
                <tile name="SOURCE" sram_amount="R1C1">
                  <cluster_info amount="1">
                    <cluster type="SRC1x1">
                      <site name="SRC0" position="R0C0">
                        <site_sram>
                          <sram basic_cell="CFG" sram_name="BIT" local_place="B0W0" owner_tile="OWNER" brick_offset="R0C-1"/>
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
        let config_image = ConfigImage {
            tiles: vec![TileConfigImage {
                tile_name: "SRC0".to_string(),
                tile_type: "SOURCE".to_string(),
                x: 0,
                y: 1,
                rows: 1,
                cols: 1,
                configs: Vec::new(),
                assignments: vec![TileBitAssignment {
                    site_name: "SRC0".to_string(),
                    cfg_name: "MODE".to_string(),
                    function_name: "ON".to_string(),
                    basic_cell: "CFG".to_string(),
                    sram_name: "BIT".to_string(),
                    row: 0,
                    col: 0,
                    value: 0,
                }],
            }],
            notes: Vec::new(),
        };

        let mut notes = Vec::new();
        let columns = build_tile_columns(&arch, &cil, &config_image, &HashMap::new(), &mut notes);

        let owner = columns
            .get(&0)
            .expect("owner column")
            .first()
            .expect("owner tile");
        let source = columns
            .get(&1)
            .expect("source column")
            .first()
            .expect("source tile");

        assert_eq!(owner.bits, vec![0]);
        assert_eq!(source.bits, vec![1]);
        assert!(notes.is_empty());
    }
}
