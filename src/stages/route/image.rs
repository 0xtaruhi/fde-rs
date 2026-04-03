use anyhow::Result;
use std::path::Path;

use crate::{
    cil::Cil,
    ir::{Design, RoutePip},
    resource::Arch,
};

use super::{
    DeviceRouteImage, RoutedNetPip,
    lookup::route_context_for_node,
    types::{RouteNode, WireInterner},
};
use crate::resource::routing::load_site_route_graphs;

pub fn materialize_route_image(
    route_pips: &[RoutedNetPip],
    arch: &Arch,
    arch_path: &Path,
    cil: &Cil,
) -> Result<DeviceRouteImage> {
    let mut wires = WireInterner::default();
    let graphs = load_site_route_graphs(arch_path, cil, &mut wires)?;
    let mut image = DeviceRouteImage::default();

    for pip in route_pips {
        let node = RouteNode::new(pip.x, pip.y, wires.intern(&pip.to_net));
        let Some(tile) = route_context_for_node(arch, cil, &node) else {
            image.notes.push(format!(
                "No tile context for route pip {} @ {},{} {} -> {}.",
                pip.net_name, pip.x, pip.y, pip.from_net, pip.to_net
            ));
            continue;
        };
        let Some(graph) = tile.graph(&graphs) else {
            image.notes.push(format!(
                "No route graph for route pip {} on {}:{}.",
                pip.net_name, tile.tile_type, tile.site_type
            ));
            continue;
        };
        let from = wires.intern(&pip.from_net);
        let to = wires.intern(&pip.to_net);
        let Some(arc) = graph
            .arcs
            .iter()
            .find(|arc| arc.from == from && arc.to == to)
        else {
            image.notes.push(format!(
                "Route pip {} @ {},{} {} -> {} does not match any {} arc.",
                pip.net_name, pip.x, pip.y, pip.from_net, pip.to_net, tile.site_type
            ));
            continue;
        };
        image
            .pips
            .push(tile.pip(pip.net_name.clone(), pip.x, pip.y, arc, &wires));
    }

    Ok(image)
}

pub fn materialize_design_route_image(
    design: &Design,
    arch: &Arch,
    arch_path: &Path,
    cil: &Cil,
) -> Result<Option<DeviceRouteImage>> {
    let route_pips = collect_design_route_pips(design);
    if route_pips.is_empty() {
        return Ok(None);
    }
    materialize_route_image(&route_pips, arch, arch_path, cil).map(Some)
}

pub fn collect_design_route_pips(design: &Design) -> Vec<RoutedNetPip> {
    let mut pips = Vec::new();
    for net in &design.nets {
        for RoutePip {
            x,
            y,
            from_net,
            to_net,
        } in &net.route_pips
        {
            pips.push(RoutedNetPip {
                net_name: net.name.clone(),
                x: *x,
                y: *y,
                from_net: from_net.clone(),
                to_net: to_net.clone(),
            });
        }
    }
    pips.sort_by(|lhs, rhs| {
        (
            lhs.net_name.as_str(),
            lhs.x,
            lhs.y,
            lhs.from_net.as_str(),
            lhs.to_net.as_str(),
        )
            .cmp(&(
                rhs.net_name.as_str(),
                rhs.x,
                rhs.y,
                rhs.from_net.as_str(),
                rhs.to_net.as_str(),
            ))
    });
    pips.dedup_by(|lhs, rhs| {
        lhs.net_name == rhs.net_name
            && lhs.x == rhs.x
            && lhs.y == rhs.y
            && lhs.from_net == rhs.from_net
            && lhs.to_net == rhs.to_net
    });
    pips
}
