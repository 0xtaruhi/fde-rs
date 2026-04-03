use super::{
    RouteNode, TilePortDef, TilePortRef, TileSide, TileStitchDb, TileWireStitch, WireId,
    WireInterner, clock::clock_spine_neighbors,
};
use crate::resource::Arch;
use anyhow::{Context, Result};
use roxmltree::Document;
use smallvec::SmallVec;
use std::{fs, path::Path};

pub(crate) fn load_tile_stitch_db(path: &Path, wires: &mut WireInterner) -> Result<TileStitchDb> {
    let xml = fs::read_to_string(path)
        .with_context(|| format!("failed to read architecture {}", path.display()))?;
    let doc = Document::parse(&xml).context("failed to parse architecture xml")?;
    let mut db = TileStitchDb::default();

    for library in doc
        .root_element()
        .children()
        .filter(|node| node.has_tag_name("library"))
    {
        for cell in library
            .children()
            .filter(|node| node.has_tag_name("cell") && node.attribute("type") == Some("TILE"))
        {
            let Some(tile_type) = cell.attribute("name") else {
                continue;
            };
            let ports = cell
                .children()
                .filter(|node| node.has_tag_name("port"))
                .filter_map(parse_tile_port_def)
                .collect::<Vec<_>>();
            if ports.is_empty() {
                continue;
            }
            let Some(contents) = cell.children().find(|node| node.has_tag_name("contents")) else {
                continue;
            };

            let mut tile = TileWireStitch::default();
            for net in contents.children().filter(|node| node.has_tag_name("net")) {
                let Some(net_name) = net.attribute("name") else {
                    continue;
                };
                let wire = wires.intern(net_name);
                let mut refs = SmallVec::<[TilePortRef; 2]>::new();
                for port_ref in net.children().filter(|node| {
                    node.has_tag_name("portRef") && node.attribute("instanceRef").is_none()
                }) {
                    let Some(port_name) = port_ref.attribute("name") else {
                        continue;
                    };
                    let Some(tile_port) = resolve_tile_port_ref(port_name, &ports) else {
                        continue;
                    };
                    if !refs.contains(&tile_port) {
                        refs.push(tile_port);
                    }
                }
                if refs.is_empty() {
                    continue;
                }
                tile.net_ports.insert(wire, refs.clone());
                for port in refs {
                    tile.port_nets
                        .entry((port.side, port.index))
                        .or_default()
                        .push(wire);
                }
            }
            db.tiles.insert(tile_type.to_string(), tile);
        }
    }

    Ok(db)
}

pub(crate) fn stitched_neighbors(
    db: &TileStitchDb,
    arch: &Arch,
    wires: &WireInterner,
    node: &RouteNode,
) -> SmallVec<[(usize, usize, WireId); 16]> {
    let mut result = SmallVec::<[(usize, usize, WireId); 16]>::new();
    result.extend(stitched_neighbors_raw(db, arch, node));
    for neighbor in clock_spine_neighbors(arch, wires, node) {
        if !result.contains(&neighbor) {
            result.push(neighbor);
        }
    }
    result
}

pub(super) fn stitched_neighbors_raw(
    db: &TileStitchDb,
    arch: &Arch,
    node: &RouteNode,
) -> SmallVec<[(usize, usize, WireId); 8]> {
    let mut result = SmallVec::new();
    let Some(tile) = arch.tile_at(node.x, node.y) else {
        return result;
    };
    let Some(tile_stitch) = db.tiles.get(tile.tile_type.as_str()) else {
        return result;
    };
    let Some(ports) = tile_stitch.net_ports.get(&node.wire) else {
        return result;
    };

    for port in ports {
        let Some((next_x, next_y, opposite_side)) = neighbor_for_port(node.x, node.y, *port) else {
            continue;
        };
        let Some(next_tile) = arch.tile_at(next_x, next_y) else {
            continue;
        };
        let Some(next_stitch) = db.tiles.get(next_tile.tile_type.as_str()) else {
            continue;
        };
        let Some(next_wires) = next_stitch.port_nets.get(&(opposite_side, port.index)) else {
            continue;
        };
        for &next_wire in next_wires {
            let neighbor = (next_x, next_y, next_wire);
            if !result.contains(&neighbor) {
                result.push(neighbor);
            }
        }
    }

    result
}

fn parse_tile_port_def(node: roxmltree::Node<'_, '_>) -> Option<TilePortDef> {
    let name = node.attribute("name")?.to_string();
    let side = match node.attribute("side")? {
        "left" => TileSide::Left,
        "right" => TileSide::Right,
        "top" => TileSide::Top,
        "bottom" => TileSide::Bottom,
        _ => return None,
    };
    let lsb = node
        .attribute("lsb")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let msb = node
        .attribute("msb")
        .and_then(|value| value.parse().ok())
        .unwrap_or(lsb);
    Some(TilePortDef {
        name,
        side,
        lsb,
        msb,
    })
}

fn resolve_tile_port_ref(name: &str, ports: &[TilePortDef]) -> Option<TilePortRef> {
    for port in ports {
        let Some(index) = port_index(name, port) else {
            continue;
        };
        return Some(TilePortRef {
            side: port.side,
            index,
        });
    }
    None
}

fn port_index(name: &str, port: &TilePortDef) -> Option<usize> {
    let suffix = name.strip_prefix(port.name.as_str())?;
    if suffix.is_empty() && port.lsb == port.msb {
        return Some(port.lsb);
    }
    let index = suffix.parse::<usize>().ok()?;
    (port.lsb..=port.msb).contains(&index).then_some(index)
}

pub(super) fn neighbor_for_port(
    x: usize,
    y: usize,
    port: TilePortRef,
) -> Option<(usize, usize, TileSide)> {
    match port.side {
        TileSide::Left => y.checked_sub(1).map(|next_y| (x, next_y, TileSide::Right)),
        TileSide::Right => y.checked_add(1).map(|next_y| (x, next_y, TileSide::Left)),
        TileSide::Top => x.checked_sub(1).map(|next_x| (next_x, y, TileSide::Bottom)),
        TileSide::Bottom => x.checked_add(1).map(|next_x| (next_x, y, TileSide::Top)),
    }
}
