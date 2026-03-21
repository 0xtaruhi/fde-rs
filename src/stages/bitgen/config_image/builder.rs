use crate::{
    cil::Cil,
    config_image::{
        accumulator::TileAccumulator,
        lookup::{find_route_sram, find_tile_sram},
        requests::derive_site_requests,
        resolve::resolve_site_config,
        types::{ConfigImage, ConfigResolution, SiteInstance, TileBitAssignment},
    },
    device::DeviceDesign,
    route_bits::DeviceRouteImage,
};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};

pub fn build_config_image(
    device: &DeviceDesign,
    cil: &Cil,
    route_image: Option<&DeviceRouteImage>,
) -> Result<ConfigImage> {
    let mut notes = vec![
        "Rust tile config image covers logic/IO/clock site SRAM and routed transmission SRAM when available."
            .to_string(),
    ];
    let mut tile_map = BTreeMap::<(String, String, String), SiteInstance>::new();

    for cell in &device.cells {
        if cell.tile_type.is_empty() || cell.site_name.is_empty() {
            continue;
        }
        let key = (
            cell.tile_name.clone(),
            cell.site_name.clone(),
            cell.site_kind.clone(),
        );
        let entry = tile_map.entry(key).or_insert_with(|| SiteInstance {
            tile_name: cell.tile_name.clone(),
            tile_type: cell.tile_type.clone(),
            site_kind: cell.site_kind.clone(),
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
        let Some(site_def) = cil.sites.get(&site.site_kind) else {
            notes.push(format!(
                "Missing CIL site definition for {} on tile {}.",
                site.site_kind, site.tile_name
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

        let requests = derive_site_requests(&site, device, site_def);
        let accumulator = tiles
            .entry((
                site.tile_name.clone(),
                site.tile_type.clone(),
                site.x,
                site.y,
            ))
            .or_insert_with(|| TileAccumulator::new(&site, tile_def.sram_rows, tile_def.sram_cols));

        for request in requests {
            match resolve_site_config(site_def, &request.cfg_name, &request.function_name) {
                ConfigResolution::Matched(bits) => {
                    accumulator.configs_mut().insert((
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
                        if let Some(owner_tile) = mapping.owner_tile.as_ref() {
                            notes.push(format!(
                                "Owner-tile remap {}:{} -> {} is recorded but not emitted yet.",
                                site.site_name, bit.cfg_name, owner_tile
                            ));
                        }
                        let Some((row, col)) = mapping.local_place else {
                            notes.push(format!(
                                "Local place is missing for {}:{}:{} on {}:{}.",
                                bit.cfg_name,
                                bit.basic_cell,
                                bit.sram_name,
                                site.tile_type,
                                site.site_name
                            ));
                            continue;
                        };
                        accumulator.insert(TileBitAssignment {
                            site_name: site.site_name.clone(),
                            cfg_name: bit.cfg_name.clone(),
                            function_name: bit.function_name.clone(),
                            basic_cell: bit.basic_cell.clone(),
                            sram_name: bit.sram_name.clone(),
                            row,
                            col,
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
            let accumulator = tiles
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
                });
            accumulator.configs_mut().insert((
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
                let Some((row, col)) = mapping.local_place else {
                    notes.push(format!(
                        "Route local place is missing for {}:{}:{} on {}:{}.",
                        pip.from_net, bit.basic_cell, bit.sram_name, pip.tile_type, pip.site_name
                    ));
                    continue;
                };
                accumulator.insert(TileBitAssignment {
                    site_name: pip.site_name.clone(),
                    cfg_name: pip.from_net.clone(),
                    function_name: pip.to_net.clone(),
                    basic_cell: bit.basic_cell.clone(),
                    sram_name: bit.sram_name.clone(),
                    row,
                    col,
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
