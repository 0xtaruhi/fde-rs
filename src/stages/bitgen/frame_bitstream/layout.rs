use crate::{
    cil::{Cil, TileSite, TileSiteSram},
    config_image::{ConfigImage, default_site_bits, find_site_sram},
    resource::{Arch, TileInstance},
};

use super::model::{DEFAULT_FILL_BIT, TileColumns, TileFrameImage};

pub(crate) fn build_tile_columns(
    arch: &Arch,
    cil: &Cil,
    config_image: &ConfigImage,
    notes: &mut Vec<String>,
) -> TileColumns {
    let mut tiles_by_name = build_arch_tile_images(arch, cil, notes);
    apply_config_assignments(&mut tiles_by_name, config_image, notes);

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
    apply_default_tile_bits(arch, cil, &mut tiles_by_name, notes);
    tiles_by_name
}

fn apply_default_tile_bits(
    arch: &Arch,
    cil: &Cil,
    tiles_by_name: &mut std::collections::BTreeMap<String, TileFrameImage>,
    notes: &mut Vec<String>,
) {
    let tiles_by_bit = arch
        .tiles
        .values()
        .map(|tile| {
            (
                (tile.bit_x as isize, tile.bit_y as isize),
                tile.name.clone(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut assignments = Vec::<DefaultTileBit>::new();

    for tile in arch.tiles.values() {
        let Some(tile_def) = cil.tiles.get(&tile.tile_type) else {
            continue;
        };

        for cluster in &tile_def.clusters {
            let Some(site_def) = cil.sites.get(&cluster.site_type) else {
                continue;
            };
            let default_bits = default_site_bits(site_def);
            if default_bits.is_empty() {
                continue;
            }
            for site in &cluster.sites {
                for bit in &default_bits {
                    if let Some(mapping) = find_site_sram(site, &bit.basic_cell, &bit.sram_name) {
                        push_default_assignment(
                            &mut assignments,
                            &tiles_by_bit,
                            tile,
                            mapping,
                            bit.value,
                            notes,
                        );
                    }
                }
            }
        }

        for transmission in &tile_def.transmissions {
            for site in &transmission.sites {
                collect_default_route_bits(
                    &mut assignments,
                    &tiles_by_bit,
                    arch,
                    tile,
                    site,
                    cil,
                    notes,
                );
            }
        }
    }

    for assignment in assignments {
        let Some(image) = tiles_by_name.get_mut(&assignment.tile_name) else {
            notes.push(format!(
                "Default SRAM target tile {} is missing from the architecture layout.",
                assignment.tile_name
            ));
            continue;
        };
        if assignment.row >= image.rows || assignment.col >= image.cols {
            notes.push(format!(
                "Default SRAM bit {}:{} falls outside {}:{} bounds {}x{}.",
                assignment.row,
                assignment.col,
                assignment.tile_name,
                image.tile_type,
                image.rows,
                image.cols
            ));
            continue;
        }
        image.bits[assignment.row * image.cols + assignment.col] = assignment.value & 1;
    }
}

fn collect_default_route_bits(
    assignments: &mut Vec<DefaultTileBit>,
    tiles_by_bit: &std::collections::BTreeMap<(isize, isize), String>,
    arch: &Arch,
    tile: &TileInstance,
    site: &TileSite,
    cil: &Cil,
    notes: &mut Vec<String>,
) {
    let mut seen_basic_cells = std::collections::BTreeSet::<String>::new();
    for sram in &site.srams {
        if sram.basic_cell.is_empty() || !seen_basic_cells.insert(sram.basic_cell.clone()) {
            continue;
        }
        let Some(element_name) = arch.site_instance_element(&site.site_type, &sram.basic_cell)
        else {
            continue;
        };
        let Some(element) = cil.elements.get(element_name) else {
            continue;
        };
        for default in &element.default_srams {
            let Some(mapping) = find_site_sram(site, &sram.basic_cell, &default.name) else {
                continue;
            };
            push_default_assignment(
                assignments,
                tiles_by_bit,
                tile,
                mapping,
                default.value,
                notes,
            );
        }
    }
}

fn push_default_assignment(
    assignments: &mut Vec<DefaultTileBit>,
    tiles_by_bit: &std::collections::BTreeMap<(isize, isize), String>,
    tile: &TileInstance,
    mapping: &TileSiteSram,
    value: u8,
    notes: &mut Vec<String>,
) {
    let target_name = match target_tile_name(tiles_by_bit, tile, mapping, notes) {
        Some(name) => name,
        None => return,
    };
    let Some((row, col)) = mapping.local_place else {
        notes.push(format!(
            "Default SRAM {}:{} on {} is missing a local_place mapping.",
            mapping.basic_cell, mapping.sram_name, tile.name
        ));
        return;
    };
    assignments.push(DefaultTileBit {
        tile_name: target_name,
        row,
        col,
        value,
    });
}

fn target_tile_name(
    tiles_by_bit: &std::collections::BTreeMap<(isize, isize), String>,
    tile: &TileInstance,
    mapping: &TileSiteSram,
    notes: &mut Vec<String>,
) -> Option<String> {
    let Some((row_offset, col_offset)) = mapping.brick_offset else {
        return Some(tile.name.clone());
    };
    let target = (
        tile.bit_x as isize + row_offset,
        tile.bit_y as isize + col_offset,
    );
    match tiles_by_bit.get(&target) {
        Some(name) => Some(name.clone()),
        None => {
            notes.push(format!(
                "Default SRAM {}:{} from {} references missing bit-tile offset R{}C{}.",
                mapping.basic_cell, mapping.sram_name, tile.name, row_offset, col_offset
            ));
            None
        }
    }
}

fn apply_config_assignments(
    tiles_by_name: &mut std::collections::BTreeMap<String, TileFrameImage>,
    config_image: &ConfigImage,
    notes: &mut Vec<String>,
) {
    for tile in &config_image.tiles {
        let Some(image) = tiles_by_name.get_mut(&tile.tile_name) else {
            notes.push(format!(
                "Config image tile {} does not exist in the architecture layout.",
                tile.tile_name
            ));
            continue;
        };

        if image.tile_type != tile.tile_type {
            notes.push(format!(
                "Config image tile {} changed type from {} to {}.",
                tile.tile_name, image.tile_type, tile.tile_type
            ));
        }
        if image.rows != tile.rows || image.cols != tile.cols {
            notes.push(format!(
                "Config image tile {} shape {}x{} does not match CIL shape {}x{}.",
                tile.tile_name, tile.rows, tile.cols, image.rows, image.cols
            ));
        }

        for assignment in &tile.assignments {
            if assignment.row >= image.rows || assignment.col >= image.cols {
                notes.push(format!(
                    "Config bit {}:{} for tile {} falls outside {}x{} SRAM bounds.",
                    assignment.row, assignment.col, tile.tile_name, image.rows, image.cols
                ));
                continue;
            }
            let offset = assignment.row * image.cols + assignment.col;
            image.bits[offset] = assignment.value & 1;
        }
    }
}

#[derive(Debug)]
struct DefaultTileBit {
    tile_name: String,
    row: usize,
    col: usize,
    value: u8,
}

#[cfg(test)]
mod tests {
    use super::build_tile_columns;
    use crate::{
        cil::{
            Cil, ClusterDef, ElementDef, SiteConfigElement, SiteDef, SiteFunction,
            SiteFunctionSram, SramSetting, TileCluster, TileDef, TileSite, TileSiteSram,
            TileTransmission, TransmissionDef,
        },
        config_image::ConfigImage,
        resource::{Arch, TileInstance},
    };
    use std::collections::BTreeMap;

    #[test]
    fn default_site_and_route_bits_seed_tile_images_before_explicit_assignments() {
        let arch = Arch {
            tiles: BTreeMap::from([(
                (0, 0),
                TileInstance {
                    name: "T0".to_string(),
                    tile_type: "CENTER".to_string(),
                    bit_x: 0,
                    bit_y: 0,
                    ..TileInstance::default()
                },
            )]),
            site_instance_types: BTreeMap::from([(
                "GSB_CNT".to_string(),
                BTreeMap::from([("BUF".to_string(), "BUF".to_string())]),
            )]),
            ..Arch::default()
        };
        let cil = Cil {
            sites: BTreeMap::from([(
                "SLICE".to_string(),
                SiteDef {
                    name: "SLICE".to_string(),
                    config_elements: vec![SiteConfigElement {
                        name: "MUX".to_string(),
                        functions: vec![SiteFunction {
                            name: "ON".to_string(),
                            is_default: true,
                            srams: vec![SiteFunctionSram {
                                basic_cell: "SELMUX".to_string(),
                                name: "P0".to_string(),
                                content: 0,
                                address: None,
                            }],
                            ..SiteFunction::default()
                        }],
                    }],
                },
            )]),
            clusters: BTreeMap::from([(
                "SLICE1x1".to_string(),
                ClusterDef {
                    name: "SLICE1x1".to_string(),
                    site_type: "SLICE".to_string(),
                },
            )]),
            transmissions: BTreeMap::from([(
                "GSB1x1".to_string(),
                TransmissionDef {
                    name: "GSB1x1".to_string(),
                    site_type: "GSB_CNT".to_string(),
                },
            )]),
            elements: BTreeMap::from([(
                "BUF".to_string(),
                ElementDef {
                    name: "BUF".to_string(),
                    default_srams: vec![SramSetting {
                        name: "EN".to_string(),
                        value: 0,
                        defaulted: true,
                    }],
                    ..ElementDef::default()
                },
            )]),
            tiles: BTreeMap::from([(
                "CENTER".to_string(),
                TileDef {
                    name: "CENTER".to_string(),
                    sram_rows: 2,
                    sram_cols: 2,
                    clusters: vec![TileCluster {
                        cluster_name: "SLICE1x1".to_string(),
                        site_type: "SLICE".to_string(),
                        sites: vec![TileSite {
                            name: "S0".to_string(),
                            site_type: "SLICE".to_string(),
                            srams: vec![TileSiteSram {
                                basic_cell: "SELMUX".to_string(),
                                sram_name: "P0".to_string(),
                                local_place: Some((0, 0)),
                                owner_tile: None,
                                brick_offset: None,
                            }],
                            ..TileSite::default()
                        }],
                        ..TileCluster::default()
                    }],
                    transmissions: vec![TileTransmission {
                        transmission_name: "GSB1x1".to_string(),
                        site_type: "GSB_CNT".to_string(),
                        sites: vec![TileSite {
                            name: "GSB0".to_string(),
                            site_type: "GSB_CNT".to_string(),
                            srams: vec![TileSiteSram {
                                basic_cell: "BUF".to_string(),
                                sram_name: "EN".to_string(),
                                local_place: Some((1, 1)),
                                owner_tile: None,
                                brick_offset: None,
                            }],
                            ..TileSite::default()
                        }],
                        ..TileTransmission::default()
                    }],
                    ..TileDef::default()
                },
            )]),
            ..Cil::default()
        };

        let columns = build_tile_columns(&arch, &cil, &ConfigImage::default(), &mut Vec::new());
        let image = &columns[&0][0];
        assert_eq!(image.bits, vec![0, 1, 1, 0]);
    }
}
