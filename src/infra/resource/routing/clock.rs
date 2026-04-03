use super::{RouteNode, WireId, WireInterner};
use crate::resource::{Arch, TileInstance, TileKind};
use smallvec::SmallVec;

pub(super) fn clock_spine_neighbors(
    arch: &Arch,
    wires: &WireInterner,
    node: &RouteNode,
) -> SmallVec<[(usize, usize, WireId); 16]> {
    let mut result = SmallVec::new();
    let wire_name = wires.resolve(node.wire);

    if let Some(index) = parse_indexed_wire_suffix(wire_name, "CLKB_GCLK")
        && let Some(tile) = unique_tile_by_kind(arch, TileKind::ClockCenter)
        && let Some(next_wire) = wires.id_indexed("CLKC_GCLK", index)
    {
        result.push((tile.logic_x, tile.logic_y, next_wire));
    }
    if let Some(index) = parse_indexed_wire_suffix(wire_name, "CLKC_VGCLK") {
        for tile in tiles_by_kind(arch, TileKind::ClockVertical) {
            if let Some(next_wire) = wires.id_indexed("CLKV_VGCLK", index) {
                result.push((tile.logic_x, tile.logic_y, next_wire));
            }
        }
    }
    if let Some(index) = parse_indexed_wire_suffix(wire_name, "CLKV_GCLK_BUFL") {
        for tile in logic_tiles_same_row_side(arch, node.x, node.y, ClockSide::LeftOrCenter) {
            if let Some(next_wire) = wires.id_indexed("GCLK", index) {
                result.push((tile.logic_x, tile.logic_y, next_wire));
            }
        }
    }
    if let Some(index) = parse_indexed_wire_suffix(wire_name, "CLKV_GCLK_BUFR") {
        for tile in logic_tiles_same_row_side(arch, node.x, node.y, ClockSide::RightOrCenter) {
            if let Some(next_wire) = wires.id_indexed("GCLK", index) {
                result.push((tile.logic_x, tile.logic_y, next_wire));
            }
        }
    }
    if let Some(index) = parse_indexed_wire_suffix(wire_name, "GBRKV_GCLKW")
        && let Some(next_wire) = wires.id_indexed("GBRKV_GCLKE", index)
    {
        result.push((node.x, node.y, next_wire));
    }
    if let Some(index) = parse_indexed_wire_suffix(wire_name, "GBRKV_GCLKE")
        && let Some(next_wire) = wires.id_indexed("GBRKV_GCLKW", index)
    {
        result.push((node.x, node.y, next_wire));
    }

    result
}

#[derive(Clone, Copy)]
enum ClockSide {
    LeftOrCenter,
    RightOrCenter,
}

fn logic_tiles_same_row_side(
    arch: &Arch,
    x: usize,
    center_y: usize,
    side: ClockSide,
) -> Vec<&TileInstance> {
    let mut tiles = arch
        .tiles
        .values()
        .filter(|tile| tile.logic_x == x && tile.kind() == TileKind::Logic)
        .filter(|tile| match side {
            ClockSide::LeftOrCenter => tile.logic_y <= center_y,
            ClockSide::RightOrCenter => tile.logic_y >= center_y,
        })
        .collect::<Vec<_>>();
    tiles.sort_by_key(|tile| tile.logic_y);
    tiles
}

fn unique_tile_by_kind(arch: &Arch, kind: TileKind) -> Option<&TileInstance> {
    arch.tiles.values().find(|tile| tile.kind() == kind)
}

fn tiles_by_kind(arch: &Arch, kind: TileKind) -> impl Iterator<Item = &TileInstance> {
    arch.tiles.values().filter(move |tile| tile.kind() == kind)
}

fn parse_indexed_wire_suffix(raw: &str, prefix: &str) -> Option<usize> {
    raw.strip_prefix(prefix)?.parse::<usize>().ok()
}
