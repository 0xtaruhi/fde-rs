use anyhow::{Context, Result};
use roxmltree::Document;
use smallvec::SmallVec;
use std::{collections::HashMap, fs, path::Path};

use crate::resource::Arch;

use super::{
    types::{RouteNode, WireId, WireInterner},
    wire::parse_wire_index,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TileSide {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TilePortRef {
    side: TileSide,
    index: usize,
}

#[derive(Debug, Clone, Default)]
struct TileWireStitch {
    net_ports: HashMap<WireId, SmallVec<[TilePortRef; 2]>>,
    port_nets: HashMap<(TileSide, usize), SmallVec<[WireId; 2]>>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TileStitchDb {
    tiles: HashMap<String, TileWireStitch>,
}

#[derive(Debug, Clone)]
struct TilePortDef {
    name: String,
    side: TileSide,
    lsb: usize,
    msb: usize,
}

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

pub(crate) fn clock_spine_neighbors(
    arch: &Arch,
    wires: &mut WireInterner,
    node: &RouteNode,
) -> SmallVec<[(usize, usize, WireId); 16]> {
    let mut result = SmallVec::new();
    let clkb_index = {
        let net = wires.resolve(node.wire);
        net.strip_prefix("CLKB_GCLK").and_then(parse_wire_index)
    };
    if let Some(index) = clkb_index
        && let Some(tile) = unique_tile_by_type_name(arch, "CLKC")
    {
        result.push((
            tile.logic_x,
            tile.logic_y,
            wires.intern_indexed("CLKC_GCLK", index),
        ));
    }

    let clkc_index = {
        let net = wires.resolve(node.wire);
        net.strip_prefix("CLKC_VGCLK").and_then(parse_wire_index)
    };
    if let Some(index) = clkc_index {
        for tile in tiles_by_type_name(arch, "CLKV") {
            result.push((
                tile.logic_x,
                tile.logic_y,
                wires.intern_indexed("CLKV_VGCLK", index),
            ));
        }
    }

    let clkv_left_index = {
        let net = wires.resolve(node.wire);
        net.strip_prefix("CLKV_GCLK_BUFL")
            .and_then(parse_wire_index)
    };
    if let Some(index) = clkv_left_index {
        for tile in logic_tiles_same_row_side(arch, node.x, node.y, ClockSide::LeftOrCenter) {
            result.push((
                tile.logic_x,
                tile.logic_y,
                wires.intern_indexed("GCLK", index),
            ));
        }
    }

    let clkv_right_index = {
        let net = wires.resolve(node.wire);
        net.strip_prefix("CLKV_GCLK_BUFR")
            .and_then(parse_wire_index)
    };
    if let Some(index) = clkv_right_index {
        for tile in logic_tiles_same_row_side(arch, node.x, node.y, ClockSide::RightOrCenter) {
            result.push((
                tile.logic_x,
                tile.logic_y,
                wires.intern_indexed("GCLK", index),
            ));
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

fn neighbor_for_port(x: usize, y: usize, port: TilePortRef) -> Option<(usize, usize, TileSide)> {
    match port.side {
        TileSide::Left => y.checked_sub(1).map(|next_y| (x, next_y, TileSide::Right)),
        TileSide::Right => y.checked_add(1).map(|next_y| (x, next_y, TileSide::Left)),
        TileSide::Top => x.checked_sub(1).map(|next_x| (next_x, y, TileSide::Bottom)),
        TileSide::Bottom => x.checked_add(1).map(|next_x| (next_x, y, TileSide::Top)),
    }
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
) -> Vec<&crate::resource::TileInstance> {
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

fn unique_tile_by_type_name<'a>(
    arch: &'a Arch,
    tile_type: &str,
) -> Option<&'a crate::resource::TileInstance> {
    arch.tiles.values().find(|tile| tile.tile_type == tile_type)
}

fn tiles_by_type_name<'a>(
    arch: &'a Arch,
    tile_type: &'a str,
) -> impl Iterator<Item = &'a crate::resource::TileInstance> + 'a {
    arch.tiles
        .values()
        .filter(move |tile| tile.tile_type == tile_type)
}

#[cfg(test)]
mod tests {
    use super::{TileSide, load_tile_stitch_db, stitched_neighbors};
    use crate::{
        resource::{Arch, load_arch},
        stages::bitgen::route_bits::types::{RouteNode, WireInterner},
    };
    use std::{collections::BTreeMap, fs};
    use tempfile::NamedTempFile;

    #[test]
    fn parses_tile_port_stitching_from_minimal_architecture() {
        let xml = r#"
        <architecture>
          <library name="tiles">
            <cell name="LEFT" type="TILE">
              <port name="right" msb="0" lsb="0" side="right"/>
              <contents>
                <net name="LEFT_E0">
                  <portRef name="right0"/>
                </net>
              </contents>
            </cell>
            <cell name="CENTER" type="TILE">
              <port name="left" msb="0" lsb="0" side="left"/>
              <contents>
                <net name="W0">
                  <portRef name="left0"/>
                </net>
              </contents>
            </cell>
          </library>
        </architecture>
        "#;
        let file = NamedTempFile::new().expect("temp arch");
        fs::write(file.path(), xml).expect("write arch");
        let mut wires = WireInterner::default();
        let db = load_tile_stitch_db(file.path(), &mut wires).expect("load stitch db");

        let arch = Arch {
            width: 1,
            height: 2,
            tiles: BTreeMap::from([
                (
                    (0, 0),
                    crate::resource::TileInstance {
                        name: "L0".to_string(),
                        tile_type: "LEFT".to_string(),
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
                    crate::resource::TileInstance {
                        name: "C0".to_string(),
                        tile_type: "CENTER".to_string(),
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

        let node = RouteNode::new(0, 0, wires.intern("LEFT_E0"));
        let neighbors = stitched_neighbors(&db, &arch, &node);

        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0].0, 0);
        assert_eq!(neighbors[0].1, 1);
        assert_eq!(wires.resolve(neighbors[0].2), "W0");
    }

    #[test]
    fn real_arch_stitching_matches_llh_and_edge_port_mappings() {
        let Some(bundle) = crate::resource::ResourceBundle::discover_from(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        )
        .ok() else {
            return;
        };
        let arch_path = bundle.root.join("fdp3p7_arch.xml");
        if !arch_path.exists() {
            return;
        }

        let arch = load_arch(&arch_path).expect("load arch");
        let mut wires = WireInterner::default();
        let db = load_tile_stitch_db(&arch_path, &mut wires).expect("load stitch db");

        let right_llh = stitched_neighbors(
            &db,
            &arch,
            &RouteNode::new(4, 53, wires.intern("RIGHT_LLH3")),
        );
        assert!(
            right_llh
                .iter()
                .any(|&(x, y, wire)| x == 4 && y == 52 && wires.resolve(wire) == "LLH4")
        );

        let right_h6 = stitched_neighbors(
            &db,
            &arch,
            &RouteNode::new(4, 53, wires.intern("RIGHT_H6W10")),
        );
        assert!(
            right_h6
                .iter()
                .any(|&(x, y, wire)| x == 4 && y == 52 && wires.resolve(wire) == "H6D10")
        );

        let left_short =
            stitched_neighbors(&db, &arch, &RouteNode::new(5, 1, wires.intern("LEFT_E13")));
        assert!(
            left_short
                .iter()
                .any(|&(x, y, wire)| x == 5 && y == 2 && wires.resolve(wire) == "W13")
        );

        let left_h6 =
            stitched_neighbors(&db, &arch, &RouteNode::new(5, 1, wires.intern("LEFT_H6E3")));
        assert!(
            left_h6
                .iter()
                .any(|&(x, y, wire)| x == 5 && y == 2 && wires.resolve(wire) == "H6A3")
        );
    }

    #[test]
    fn tile_side_neighbor_directions_are_consistent() {
        let left = super::neighbor_for_port(
            3,
            4,
            super::TilePortRef {
                side: TileSide::Left,
                index: 7,
            },
        );
        let right = super::neighbor_for_port(
            3,
            4,
            super::TilePortRef {
                side: TileSide::Right,
                index: 7,
            },
        );
        let top = super::neighbor_for_port(
            3,
            4,
            super::TilePortRef {
                side: TileSide::Top,
                index: 7,
            },
        );
        let bottom = super::neighbor_for_port(
            3,
            4,
            super::TilePortRef {
                side: TileSide::Bottom,
                index: 7,
            },
        );

        assert_eq!(left, Some((3, 3, TileSide::Right)));
        assert_eq!(right, Some((3, 5, TileSide::Left)));
        assert_eq!(top, Some((2, 4, TileSide::Bottom)));
        assert_eq!(bottom, Some((4, 4, TileSide::Top)));
    }
}
