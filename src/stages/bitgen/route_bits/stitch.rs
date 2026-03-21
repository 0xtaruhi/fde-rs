use crate::resource::{Arch, TileInstance};
use smallvec::SmallVec;

use super::{
    types::RouteNode,
    wire::{parse_indexed_wire, parse_wire_index},
};

pub(crate) fn stitched_neighbors(
    arch: &Arch,
    node: &RouteNode,
) -> SmallVec<[(usize, usize, String); 2]> {
    let mut result = SmallVec::new();
    let Some((direction, index)) = parse_indexed_wire(&node.net) else {
        return result;
    };
    match direction.as_str() {
        "RIGHT_LLH" | "LEFT_LLH" => {
            for tile in logic_tiles_same_row(arch, node.x) {
                result.push((tile.logic_x, tile.logic_y, "LLH0".to_string()));
                result.push((tile.logic_x, tile.logic_y, "LLH6".to_string()));
            }
            return result;
        }
        "RIGHT_LLV" | "LEFT_LLV" | "TOP_LLV" | "BOT_LLV" => {
            for tile in logic_tiles_same_column(arch, node.y) {
                result.push((tile.logic_x, tile.logic_y, "LLV0".to_string()));
                result.push((tile.logic_x, tile.logic_y, "LLV6".to_string()));
            }
            return result;
        }
        "LLH" => {
            for tile in logic_tiles_same_row(arch, node.x) {
                if tile.logic_y != node.y {
                    result.push((tile.logic_x, tile.logic_y, format!("LLH{index}")));
                }
            }
            if let Some(tile) = edge_tile_same_row(arch, node.x, "LEFT") {
                result.push((tile.logic_x, tile.logic_y, format!("LEFT_LLH{index}")));
            }
            if let Some(tile) = edge_tile_same_row(arch, node.x, "RIGHT") {
                result.push((tile.logic_x, tile.logic_y, format!("RIGHT_LLH{index}")));
            }
            return result;
        }
        "LLV" => {
            for tile in logic_tiles_same_column(arch, node.y) {
                if tile.logic_x != node.x {
                    result.push((tile.logic_x, tile.logic_y, "LLV0".to_string()));
                    result.push((tile.logic_x, tile.logic_y, "LLV6".to_string()));
                }
            }
            for tile_type in ["TOP", "BOT", "LEFT", "RIGHT"] {
                if let Some(tile) = edge_tile_same_column(arch, node.y, tile_type) {
                    result.push((
                        tile.logic_x,
                        tile.logic_y,
                        format!("{tile_type}_LLV{index}"),
                    ));
                }
            }
            return result;
        }
        _ => {}
    }
    match direction.as_str() {
        "E" => push_stitched_target(arch, &mut result, node, 0, 1, "W", index, 1),
        "W" => push_stitched_target(arch, &mut result, node, 0, -1, "E", index, 1),
        "N" => push_stitched_target(arch, &mut result, node, -1, 0, "S", index, 1),
        "S" => push_stitched_target(arch, &mut result, node, 1, 0, "N", index, 1),
        "H6E" => {
            push_stitched_target(arch, &mut result, node, 0, 1, "H6W", index, 6);
            push_stitched_target(arch, &mut result, node, 0, 1, "H6M", index, 3);
        }
        "H6M" => {
            push_stitched_target(arch, &mut result, node, 0, -1, "H6E", index, 3);
            push_stitched_target(arch, &mut result, node, 0, 1, "H6W", index, 3);
        }
        "H6W" => {
            push_stitched_target(arch, &mut result, node, 0, -1, "H6E", index, 6);
            push_stitched_target(arch, &mut result, node, 0, -1, "H6M", index, 3);
        }
        "V6N" => push_stitched_target(arch, &mut result, node, -1, 0, "V6S", index, 6),
        "V6S" => push_stitched_target(arch, &mut result, node, 1, 0, "V6N", index, 6),
        _ => {}
    }
    result
}

fn decorate_target_wire(tile_type: &str, prefix: &str, index: usize) -> String {
    match (tile_type, prefix) {
        ("LEFT", "E") => format!("LEFT_E{index}"),
        ("RIGHT", "W") => format!("RIGHT_W{index}"),
        ("TOP", "S") => format!("TOP_S{index}"),
        ("BOT", "N") => format!("BOT_N{index}"),
        ("LEFT", "H6E") => format!("LEFT_H6E{index}"),
        ("LEFT", "H6M") => format!("LEFT_H6M{index}"),
        ("RIGHT", "H6W") => format!("RIGHT_H6W{index}"),
        ("RIGHT", "H6M") => format!("RIGHT_H6M{index}"),
        ("TOP", "V6S") => format!("TOP_V6S{index}"),
        ("BOT", "V6N") => format!("BOT_V6N{index}"),
        _ => format!("{prefix}{index}"),
    }
}

fn push_stitched_target(
    arch: &Arch,
    result: &mut SmallVec<[(usize, usize, String); 2]>,
    node: &RouteNode,
    dx: isize,
    dy: isize,
    target_prefix: &str,
    index: usize,
    span: usize,
) {
    let next_x = node.x as isize + dx * span as isize;
    let next_y = node.y as isize + dy * span as isize;
    if next_x < 0 || next_y < 0 {
        return;
    }
    let next_x = next_x as usize;
    let next_y = next_y as usize;
    let Some(target_tile) = arch.tile_at(next_x, next_y) else {
        return;
    };
    let target_net = decorate_target_wire(&target_tile.tile_type, target_prefix, index);
    let neighbor = (next_x, next_y, target_net);
    if !result.contains(&neighbor) {
        result.push(neighbor);
    }
}

pub(crate) fn clock_spine_neighbors(
    arch: &Arch,
    node: &RouteNode,
) -> SmallVec<[(usize, usize, String); 64]> {
    let mut result = SmallVec::new();

    if let Some(index) = node
        .net
        .strip_prefix("CLKB_GCLK")
        .and_then(parse_wire_index)
        && let Some(tile) = unique_tile_of_type(arch, "CLKC")
    {
        result.push((tile.logic_x, tile.logic_y, format!("CLKC_GCLK{index}")));
    }

    if let Some(index) = node
        .net
        .strip_prefix("CLKC_VGCLK")
        .and_then(parse_wire_index)
    {
        for tile in tiles_of_type(arch, "CLKV") {
            result.push((tile.logic_x, tile.logic_y, format!("CLKV_VGCLK{index}")));
        }
    }

    if let Some(index) = node
        .net
        .strip_prefix("CLKV_GCLK_BUFL")
        .and_then(parse_wire_index)
    {
        for tile in logic_tiles_same_row_side(arch, node.x, node.y, ClockSide::LeftOrCenter) {
            result.push((tile.logic_x, tile.logic_y, format!("GCLK{index}")));
        }
    }

    if let Some(index) = node
        .net
        .strip_prefix("CLKV_GCLK_BUFR")
        .and_then(parse_wire_index)
    {
        for tile in logic_tiles_same_row_side(arch, node.x, node.y, ClockSide::RightOrCenter) {
            result.push((tile.logic_x, tile.logic_y, format!("GCLK{index}")));
        }
    }

    if node.net == "CLKB_LLH1" {
        for tile in tiles_same_x_of_type(arch, node.x, "BOT", |candidate_y| candidate_y < node.y) {
            result.push((tile.logic_x, tile.logic_y, "BOT_LLH6".to_string()));
        }
    }

    if node.net == "CLKB_LLH4"
        && let Some(tile) = unique_tile_of_type(arch, "LL")
    {
        result.push((tile.logic_x, tile.logic_y, "LL_LLH4".to_string()));
    }

    if node.net == "LL_H6B5" {
        for tile in tiles_same_x_of_type(arch, node.x, "BOT", |candidate_y| candidate_y > node.y) {
            result.push((tile.logic_x, tile.logic_y, "BOT_H6C5".to_string()));
        }
    }

    if let Some(index) = node.net.strip_prefix("BOT_V6A").and_then(parse_wire_index) {
        for tile in logic_tiles_same_column_leftward(arch, node.x, node.y) {
            result.push((tile.logic_x, tile.logic_y, format!("V6M{index}")));
        }
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
        .filter(|tile| tile.logic_x == x && tile.tile_type == "CENTER")
        .filter(|tile| match side {
            ClockSide::LeftOrCenter => tile.logic_y <= center_y,
            ClockSide::RightOrCenter => tile.logic_y >= center_y,
        })
        .collect::<Vec<_>>();
    tiles.sort_by_key(|tile| tile.logic_y);
    tiles
}

fn logic_tiles_same_row(arch: &Arch, x: usize) -> Vec<&TileInstance> {
    let mut tiles = arch
        .tiles
        .values()
        .filter(|tile| tile.logic_x == x && tile.tile_type == "CENTER")
        .collect::<Vec<_>>();
    tiles.sort_by_key(|tile| tile.logic_y);
    tiles
}

fn logic_tiles_same_column(arch: &Arch, y: usize) -> Vec<&TileInstance> {
    let mut tiles = arch
        .tiles
        .values()
        .filter(|tile| tile.logic_y == y && tile.tile_type == "CENTER")
        .collect::<Vec<_>>();
    tiles.sort_by_key(|tile| tile.logic_x);
    tiles
}

fn edge_tile_same_row<'a>(arch: &'a Arch, x: usize, tile_type: &str) -> Option<&'a TileInstance> {
    arch.tiles
        .values()
        .find(|tile| tile.logic_x == x && tile.tile_type == tile_type)
}

fn edge_tile_same_column<'a>(
    arch: &'a Arch,
    y: usize,
    tile_type: &str,
) -> Option<&'a TileInstance> {
    arch.tiles
        .values()
        .find(|tile| tile.logic_y == y && tile.tile_type == tile_type)
}

fn logic_tiles_same_column_leftward(arch: &Arch, center_x: usize, y: usize) -> Vec<&TileInstance> {
    let mut tiles = arch
        .tiles
        .values()
        .filter(|tile| tile.logic_y == y && tile.tile_type == "CENTER" && tile.logic_x <= center_x)
        .collect::<Vec<_>>();
    tiles.sort_by_key(|tile| tile.logic_x);
    tiles
}

fn tiles_same_x_of_type<'a, F>(
    arch: &'a Arch,
    x: usize,
    tile_type: &str,
    predicate: F,
) -> Vec<&'a TileInstance>
where
    F: Fn(usize) -> bool,
{
    let mut tiles = arch
        .tiles
        .values()
        .filter(|tile| tile.logic_x == x && tile.tile_type == tile_type && predicate(tile.logic_y))
        .collect::<Vec<_>>();
    tiles.sort_by_key(|tile| tile.logic_y);
    tiles
}

fn unique_tile_of_type<'a>(arch: &'a Arch, tile_type: &str) -> Option<&'a TileInstance> {
    arch.tiles.values().find(|tile| tile.tile_type == tile_type)
}

fn tiles_of_type<'a>(arch: &'a Arch, tile_type: &str) -> impl Iterator<Item = &'a TileInstance> {
    arch.tiles
        .values()
        .filter(move |tile| tile.tile_type == tile_type)
}
