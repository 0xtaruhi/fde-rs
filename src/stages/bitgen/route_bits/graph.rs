use crate::cil::{Cil, ElementPath};
use anyhow::Result;
use roxmltree::Document;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use super::types::{RouteBit, SiteRouteArc, SiteRouteGraph};

pub(crate) fn load_site_route_graphs(
    path: &Path,
    cil: &Cil,
) -> Result<BTreeMap<String, SiteRouteGraph>> {
    let xml = fs::read_to_string(path)?;
    let doc = Document::parse(&xml)?;
    let relevant = cil
        .tiles
        .values()
        .flat_map(|tile| {
            tile.transmissions
                .iter()
                .map(|transmission| transmission.site_type.clone())
        })
        .collect::<BTreeSet<_>>();
    let mut graphs = BTreeMap::new();

    for library in doc
        .root_element()
        .children()
        .filter(|node| node.has_tag_name("library") && node.attribute("name") == Some("block"))
    {
        for cell in library.children().filter(|node| node.has_tag_name("cell")) {
            let Some(name) = cell.attribute("name") else {
                continue;
            };
            if !relevant.contains(name) {
                continue;
            }
            let mut instance_types = BTreeMap::<String, String>::new();
            let mut pin_to_nets = BTreeMap::<(String, String), Vec<String>>::new();
            if let Some(contents) = cell.children().find(|node| node.has_tag_name("contents")) {
                for instance in contents
                    .children()
                    .filter(|node| node.has_tag_name("instance"))
                {
                    let Some(instance_name) = instance.attribute("name") else {
                        continue;
                    };
                    let cell_ref = instance
                        .attribute("cellRef")
                        .unwrap_or_default()
                        .to_string();
                    instance_types.insert(instance_name.to_string(), cell_ref);
                }
                for net in contents.children().filter(|node| node.has_tag_name("net")) {
                    let Some(net_name) = net.attribute("name") else {
                        continue;
                    };
                    for port_ref in net.children().filter(|node| node.has_tag_name("portRef")) {
                        let Some(instance_name) = port_ref.attribute("instanceRef") else {
                            continue;
                        };
                        let pin_name = port_ref.attribute("name").unwrap_or_default();
                        pin_to_nets
                            .entry((instance_name.to_string(), pin_name.to_string()))
                            .or_default()
                            .push(net_name.to_string());
                    }
                }
            }

            let mut arcs = Vec::new();
            let mut seen = BTreeSet::new();
            for (instance_name, element_name) in &instance_types {
                let Some(element) = cil.elements.get(element_name) else {
                    continue;
                };
                for path in &element.paths {
                    let Some(src_nets) =
                        pin_to_nets.get(&(instance_name.clone(), path.input.clone()))
                    else {
                        continue;
                    };
                    let Some(dst_nets) =
                        pin_to_nets.get(&(instance_name.clone(), path.output.clone()))
                    else {
                        continue;
                    };
                    for src in src_nets {
                        for dst in dst_nets {
                            let key = (
                                src.clone(),
                                dst.clone(),
                                instance_name.clone(),
                                path.input.clone(),
                                path.output.clone(),
                            );
                            if !seen.insert(key) {
                                continue;
                            }
                            arcs.push(SiteRouteArc {
                                from: src.clone(),
                                to: dst.clone(),
                                basic_cell: instance_name.clone(),
                                bits: path_bits(path),
                            });
                        }
                    }
                }
            }

            let mut adjacency = BTreeMap::<String, Vec<usize>>::new();
            for (index, arc) in arcs.iter().enumerate() {
                adjacency.entry(arc.from.clone()).or_default().push(index);
            }
            graphs.insert(name.to_string(), SiteRouteGraph { arcs, adjacency });
        }
    }

    Ok(graphs)
}

fn path_bits(path: &ElementPath) -> Vec<RouteBit> {
    path.configuration
        .iter()
        .map(|setting| RouteBit {
            basic_cell: String::new(),
            sram_name: setting.name.clone(),
            value: setting.value,
        })
        .collect()
}
