use crate::{
    bitgen::{ConfigImage, TileBitAssignment, TileConfigImage},
    cil::{Cil, TileSiteSram},
    resource::{Arch, TileInstance},
};

use crate::stages::bitgen::frame_bitstream::model::TileFrameImage;

pub(super) fn apply_config_assignments(
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

pub(super) fn apply_mapped_bit(
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
