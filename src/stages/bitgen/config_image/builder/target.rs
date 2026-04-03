use crate::{
    cil::{Cil, TileSiteSram},
    resource::Arch,
};

pub(super) struct TargetAssignment {
    pub(super) tile_name: String,
    pub(super) tile_type: String,
    pub(super) x: usize,
    pub(super) y: usize,
    pub(super) rows: usize,
    pub(super) cols: usize,
    pub(super) row: usize,
    pub(super) col: usize,
}

#[derive(Clone, Copy)]
pub(super) struct SourceTileContext<'a> {
    pub(super) tile_name: &'a str,
    pub(super) tile_type: &'a str,
    pub(super) x: usize,
    pub(super) y: usize,
    pub(super) rows: usize,
    pub(super) cols: usize,
}

pub(super) fn resolve_target_assignment(
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
