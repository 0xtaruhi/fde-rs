use crate::{
    cil::{Cil, TileDef},
    resource::Arch,
};

use super::types::{RouteNode, TileRouteSite};

pub(crate) fn cil_tile_site(arch: &Arch, cil: &Cil, node: &RouteNode) -> Option<TileRouteSite> {
    let tile = cil_tile_for_node(arch, cil, node)?;
    let transmission = tile.transmissions.first()?;
    let site = transmission.sites.first()?;
    Some(TileRouteSite {
        site_name: site.name.clone(),
        site_type: transmission.site_type.clone(),
    })
}

pub(crate) fn cil_tile_for_node<'a>(
    arch: &Arch,
    cil: &'a Cil,
    node: &RouteNode,
) -> Option<&'a TileDef> {
    let tile_type = tile_type_for_node(arch, node)?;
    cil.tiles.get(tile_type)
}

pub(crate) fn tile_name_for_node(arch: &Arch, node: &RouteNode) -> Option<String> {
    Some(arch.tile_at(node.x, node.y)?.name.clone())
}

pub(crate) fn tile_type_for_node<'a>(arch: &'a Arch, node: &RouteNode) -> Option<&'a str> {
    Some(arch.tile_at(node.x, node.y)?.tile_type.as_str())
}
