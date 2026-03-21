use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};

use super::super::device::{DeviceDesign, DeviceDesignIndex};
use super::{
    accumulator::TileAccumulator,
    lookup::{find_route_sram, find_tile_sram},
    requests::{derive_site_requests, merge_site_requests},
    resolve::resolve_site_config,
    types::{ConfigImage, ConfigResolution, SiteInstance, TileBitAssignment},
};
use crate::{
    cil::{Cil, TileSiteSram},
    resource::Arch,
    route_bits::DeviceRouteImage,
};

pub fn build_config_image(
    device: &DeviceDesign,
    cil: &Cil,
    arch: Option<&Arch>,
    route_image: Option<&DeviceRouteImage>,
) -> Result<ConfigImage> {
    let mut notes = vec![
        "Rust tile config image covers logic/IO/clock site SRAM and routed transmission SRAM when available."
            .to_string(),
    ];
    let index = DeviceDesignIndex::build(device);
    let mut tile_map = BTreeMap::<(String, String, crate::domain::SiteKind), SiteInstance>::new();

    for cell in &device.cells {
        if !cell.is_sited() {
            continue;
        }
        let key = (
            cell.tile_name.clone(),
            cell.site_name.clone(),
            cell.site_kind,
        );
        let entry = tile_map.entry(key).or_insert_with(|| SiteInstance {
            tile_name: cell.tile_name.clone(),
            tile_type: cell.tile_type.clone(),
            site_kind: cell.site_kind,
            site_name: cell.site_name.clone(),
            x: cell.x,
            y: cell.y,
            z: cell.z,
            cells: Vec::new(),
        });
        entry.cells.push(cell.clone());
    }

    let mut tiles = BTreeMap::<(String, String, usize, usize), TileAccumulator>::new();
    for site in tile_map.into_values() {
        let Some(tile_def) = cil.tiles.get(&site.tile_type) else {
            notes.push(format!(
                "Missing CIL tile definition for {} on site {}.",
                site.tile_type, site.site_name
            ));
            continue;
        };
        let Some(site_def) = cil.site_def(site.site_kind) else {
            notes.push(format!(
                "Missing CIL site definition for {} on tile {}.",
                site.site_kind.as_str(),
                site.tile_name
            ));
            continue;
        };
        let Some(tile_site) = cil.tile_site(&site.tile_type, &site.site_name) else {
            notes.push(format!(
                "Missing tile-site mapping for {}:{}.",
                site.tile_type, site.site_name
            ));
            continue;
        };

        let requests = merge_site_requests(
            site_def,
            derive_site_requests(&site, device, &index, site_def),
        );
        for request in requests {
            match resolve_site_config(site_def, &request.cfg_name, &request.function_name) {
                ConfigResolution::Matched(bits) => {
                    tiles
                        .entry((
                            site.tile_name.clone(),
                            site.tile_type.clone(),
                            site.x,
                            site.y,
                        ))
                        .or_insert_with(|| {
                            TileAccumulator::new(&site, tile_def.sram_rows, tile_def.sram_cols)
                        })
                        .configs_mut()
                        .insert((
                            site.site_name.clone(),
                            request.cfg_name.clone(),
                            request.function_name.clone(),
                        ));
                    for bit in bits {
                        let Some(mapping) = find_tile_sram(tile_site, &bit) else {
                            notes.push(format!(
                                "Missing site SRAM mapping for {}:{}:{} on {}:{}.",
                                bit.cfg_name,
                                bit.basic_cell,
                                bit.sram_name,
                                site.tile_type,
                                site.site_name
                            ));
                            continue;
                        };
                        let source = SourceTileContext {
                            tile_name: &site.tile_name,
                            tile_type: &site.tile_type,
                            x: site.x,
                            y: site.y,
                            rows: tile_def.sram_rows,
                            cols: tile_def.sram_cols,
                        };
                        let Some(target) = resolve_target_assignment(
                            cil,
                            arch,
                            source,
                            mapping,
                            &format!(
                                "{}:{}:{} on {}:{}",
                                bit.cfg_name,
                                bit.basic_cell,
                                bit.sram_name,
                                site.tile_type,
                                site.site_name
                            ),
                            &mut notes,
                        ) else {
                            continue;
                        };
                        tiles
                            .entry((
                                target.tile_name.clone(),
                                target.tile_type.clone(),
                                target.x,
                                target.y,
                            ))
                            .or_insert_with(|| {
                                TileAccumulator::new_tile(
                                    &target.tile_name,
                                    &target.tile_type,
                                    target.x,
                                    target.y,
                                    target.rows,
                                    target.cols,
                                )
                            })
                            .insert(TileBitAssignment {
                                site_name: site.site_name.clone(),
                                cfg_name: bit.cfg_name.clone(),
                                function_name: bit.function_name.clone(),
                                basic_cell: bit.basic_cell.clone(),
                                sram_name: bit.sram_name.clone(),
                                row: target.row,
                                col: target.col,
                                value: bit.value,
                            });
                    }
                }
                ConfigResolution::Unmatched => notes.push(format!(
                    "Unresolved config {}={} for {}:{}.",
                    request.cfg_name, request.function_name, site.tile_type, site.site_name
                )),
            }
        }
    }

    if let Some(route_image) = route_image {
        notes.extend(route_image.notes.iter().cloned());
        for pip in &route_image.pips {
            let Some(tile_def) = cil.tiles.get(&pip.tile_type) else {
                notes.push(format!(
                    "Missing CIL tile definition for routed pip {}:{} -> {}:{}.",
                    pip.tile_type, pip.site_name, pip.from_net, pip.to_net
                ));
                continue;
            };
            let Some(tile_site) = cil.tile_transmission_site(&pip.tile_type, &pip.site_name) else {
                notes.push(format!(
                    "Missing transmission-site mapping for {}:{}.",
                    pip.tile_type, pip.site_name
                ));
                continue;
            };
            tiles
                .entry((pip.tile_name.clone(), pip.tile_type.clone(), pip.x, pip.y))
                .or_insert_with(|| {
                    TileAccumulator::new_tile(
                        &pip.tile_name,
                        &pip.tile_type,
                        pip.x,
                        pip.y,
                        tile_def.sram_rows,
                        tile_def.sram_cols,
                    )
                })
                .configs_mut()
                .insert((
                    pip.site_name.clone(),
                    pip.from_net.clone(),
                    pip.to_net.clone(),
                ));
            for bit in &pip.bits {
                let Some(mapping) = find_route_sram(tile_site, bit) else {
                    notes.push(format!(
                        "Missing route SRAM mapping for {}:{}:{} on {}:{}.",
                        pip.from_net, bit.basic_cell, bit.sram_name, pip.tile_type, pip.site_name
                    ));
                    continue;
                };
                let source = SourceTileContext {
                    tile_name: &pip.tile_name,
                    tile_type: &pip.tile_type,
                    x: pip.x,
                    y: pip.y,
                    rows: tile_def.sram_rows,
                    cols: tile_def.sram_cols,
                };
                let Some(target) = resolve_target_assignment(
                    cil,
                    arch,
                    source,
                    mapping,
                    &format!(
                        "{}:{}:{} on {}:{}",
                        pip.from_net, bit.basic_cell, bit.sram_name, pip.tile_type, pip.site_name
                    ),
                    &mut notes,
                ) else {
                    continue;
                };
                tiles
                    .entry((
                        target.tile_name.clone(),
                        target.tile_type.clone(),
                        target.x,
                        target.y,
                    ))
                    .or_insert_with(|| {
                        TileAccumulator::new_tile(
                            &target.tile_name,
                            &target.tile_type,
                            target.x,
                            target.y,
                            target.rows,
                            target.cols,
                        )
                    })
                    .insert(TileBitAssignment {
                        site_name: pip.site_name.clone(),
                        cfg_name: pip.from_net.clone(),
                        function_name: pip.to_net.clone(),
                        basic_cell: bit.basic_cell.clone(),
                        sram_name: bit.sram_name.clone(),
                        row: target.row,
                        col: target.col,
                        value: bit.value,
                    });
            }
        }
    }

    let mut image = ConfigImage {
        tiles: tiles
            .into_values()
            .map(TileAccumulator::finish)
            .filter(|tile| !tile.configs.is_empty() || !tile.assignments.is_empty())
            .collect(),
        notes,
    };
    let mut unique_notes = BTreeSet::new();
    image.notes.retain(|note| unique_notes.insert(note.clone()));
    image.tiles.sort_by(|lhs, rhs| {
        (lhs.y, lhs.x, lhs.tile_name.as_str()).cmp(&(rhs.y, rhs.x, rhs.tile_name.as_str()))
    });
    Ok(image)
}

struct TargetAssignment {
    tile_name: String,
    tile_type: String,
    x: usize,
    y: usize,
    rows: usize,
    cols: usize,
    row: usize,
    col: usize,
}

#[derive(Clone, Copy)]
struct SourceTileContext<'a> {
    tile_name: &'a str,
    tile_type: &'a str,
    x: usize,
    y: usize,
    rows: usize,
    cols: usize,
}

fn resolve_target_assignment(
    cil: &Cil,
    arch: Option<&Arch>,
    source: SourceTileContext<'_>,
    mapping: &TileSiteSram,
    context: &str,
    notes: &mut Vec<String>,
) -> Option<TargetAssignment> {
    let Some((row, col)) = mapping.local_place else {
        notes.push(format!("Local place is missing for {context}."));
        return None;
    };

    let mut target = TargetAssignment {
        tile_name: source.tile_name.to_string(),
        tile_type: source.tile_type.to_string(),
        x: source.x,
        y: source.y,
        rows: source.rows,
        cols: source.cols,
        row,
        col,
    };

    if mapping.owner_tile.is_none() {
        return Some(target);
    }

    let Some(arch) = arch else {
        notes.push(format!(
            "Owner-tile remap for {context} is deferred because architecture tiles are unavailable."
        ));
        return Some(target);
    };

    let Some((row_offset, col_offset)) = mapping.brick_offset else {
        notes.push(format!(
            "Owner-tile remap for {context} is missing a brick_offset and stays on the source tile."
        ));
        return Some(target);
    };

    let Some(target_x) = source.x.checked_add_signed(row_offset) else {
        notes.push(format!(
            "Owner-tile remap for {context} overflows row from {} by {row_offset}.",
            source.x
        ));
        return Some(target);
    };
    let Some(target_y) = source.y.checked_add_signed(col_offset) else {
        notes.push(format!(
            "Owner-tile remap for {context} overflows column from {} by {col_offset}.",
            source.y
        ));
        return Some(target);
    };

    let Some(target_tile) = arch.tile_at(target_x, target_y) else {
        notes.push(format!(
            "Owner-tile remap for {context} targets missing tile at {target_x},{target_y}."
        ));
        return Some(target);
    };

    if let Some(owner_tile) = mapping.owner_tile.as_deref()
        && target_tile.tile_type != owner_tile
    {
        notes.push(format!(
            "Owner-tile remap for {context} expected tile type {owner_tile}, found {}.",
            target_tile.tile_type
        ));
    }

    target.tile_name = target_tile.name.clone();
    target.tile_type = target_tile.tile_type.clone();
    target.x = target_tile.logic_x;
    target.y = target_tile.logic_y;
    if let Some(tile_def) = cil.tiles.get(&target.tile_type) {
        target.rows = tile_def.sram_rows;
        target.cols = tile_def.sram_cols;
    }
    Some(target)
}
