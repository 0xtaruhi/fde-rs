use crate::{cil::Cil, resource::Arch};

use super::types::{
    DeviceRoutePip, RouteBit, RouteNode, SiteRouteArc, SiteRouteGraph, SiteRouteGraphs,
    WireInterner,
};

pub(crate) struct TileRouteContext<'a> {
    pub(crate) tile_name: &'a str,
    pub(crate) tile_type: &'a str,
    pub(crate) site_name: &'a str,
    pub(crate) site_type: &'a str,
}

impl<'a> TileRouteContext<'a> {
    pub(crate) fn graph<'g>(&self, graphs: &'g SiteRouteGraphs) -> Option<&'g SiteRouteGraph> {
        graphs.get(self.site_type)
    }

    pub(crate) fn pip(
        &self,
        net_name: String,
        x: usize,
        y: usize,
        arc: &SiteRouteArc,
        wires: &WireInterner,
    ) -> DeviceRoutePip {
        DeviceRoutePip {
            net_name,
            tile_name: self.tile_name.to_string(),
            tile_type: self.tile_type.to_string(),
            site_name: self.site_name.to_string(),
            site_type: self.site_type.to_string(),
            x,
            y,
            from_net: wires.resolve(arc.from).to_string(),
            to_net: wires.resolve(arc.to).to_string(),
            bits: arc
                .bits
                .iter()
                .map(|bit| RouteBit {
                    basic_cell: arc.basic_cell.clone(),
                    sram_name: bit.sram_name.clone(),
                    value: bit.value,
                })
                .collect(),
        }
    }
}

pub(crate) fn route_context_for_node<'a>(
    arch: &'a Arch,
    cil: &'a Cil,
    node: &RouteNode,
) -> Option<TileRouteContext<'a>> {
    let tile = arch.tile_at(node.x, node.y)?;
    let tile_def = cil.tiles.get(tile.tile_type.as_str())?;
    let transmission = tile_def.transmissions.first()?;
    let site = transmission.sites.first()?;
    Some(TileRouteContext {
        tile_name: tile.name.as_str(),
        tile_type: tile.tile_type.as_str(),
        site_name: site.name.as_str(),
        site_type: transmission.site_type.as_str(),
    })
}
