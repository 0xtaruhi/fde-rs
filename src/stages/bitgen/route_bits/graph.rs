use crate::cil::{Cil, ElementPath};
use anyhow::Result;
use roxmltree::Document;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
};

use super::types::{RouteBit, SiteRouteArc, SiteRouteGraph, SiteRouteGraphs, WireId, WireInterner};

pub(crate) type SiteRouteDefaults = HashMap<String, Vec<RouteBit>>;

pub(crate) fn load_site_route_graphs(
    path: &Path,
    cil: &Cil,
    wires: &mut WireInterner,
) -> Result<SiteRouteGraphs> {
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
        .collect::<HashSet<_>>();
    let mut graphs = SiteRouteGraphs::new();

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
            let mut pin_to_nets = HashMap::<(String, String), Vec<WireId>>::new();
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
                    let wire = wires.intern(net_name);
                    for port_ref in net.children().filter(|node| node.has_tag_name("portRef")) {
                        let Some(instance_name) = port_ref.attribute("instanceRef") else {
                            continue;
                        };
                        let pin_name = port_ref.attribute("name").unwrap_or_default();
                        pin_to_nets
                            .entry((instance_name.to_string(), pin_name.to_string()))
                            .or_default()
                            .push(wire);
                    }
                }
            }

            let mut arcs = Vec::new();
            let mut seen = HashSet::new();
            for (instance_name, element_name) in &instance_types {
                let Some(element) = cil.elements.get(element_name) else {
                    continue;
                };
                for path in &element.paths {
                    let src_key = (instance_name.clone(), path.input.clone());
                    let dst_key = (instance_name.clone(), path.output.clone());
                    let Some(src_nets) = pin_to_nets.get(&src_key) else {
                        continue;
                    };
                    let Some(dst_nets) = pin_to_nets.get(&dst_key) else {
                        continue;
                    };
                    for &src in src_nets {
                        for &dst in dst_nets {
                            let key = (
                                src,
                                dst,
                                instance_name.clone(),
                                path.input.clone(),
                                path.output.clone(),
                            );
                            if !seen.insert(key) {
                                continue;
                            }
                            arcs.push(SiteRouteArc {
                                from: src,
                                to: dst,
                                basic_cell: instance_name.clone(),
                                bits: path_bits(path),
                            });
                        }
                    }
                }
            }

            let default_bits = instance_types
                .iter()
                .flat_map(|(instance_name, element_name)| {
                    cil.elements
                        .get(element_name)
                        .into_iter()
                        .flat_map(move |element| {
                            element.default_srams.iter().map(move |setting| RouteBit {
                                basic_cell: instance_name.clone(),
                                sram_name: setting.name.clone(),
                                value: setting.value,
                            })
                        })
                })
                .collect();

            let mut adjacency = HashMap::<WireId, Vec<usize>>::new();
            for (index, arc) in arcs.iter().enumerate() {
                adjacency.entry(arc.from).or_default().push(index);
            }
            graphs.insert(
                name.to_string(),
                SiteRouteGraph {
                    arcs,
                    adjacency,
                    default_bits,
                },
            );
        }
    }

    Ok(graphs)
}

pub(crate) fn load_site_route_defaults(path: &Path, cil: &Cil) -> Result<SiteRouteDefaults> {
    let mut wires = WireInterner::default();
    let graphs = load_site_route_graphs(path, cil, &mut wires)?;
    Ok(graphs
        .into_iter()
        .map(|(site_type, graph)| (site_type, graph.default_bits))
        .collect())
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

#[cfg(test)]
mod tests {
    use super::load_site_route_defaults;
    use crate::cil::parse_cil_str;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn extracts_default_transmission_bits_from_arch_instances() {
        let cil = parse_cil_str(
            r##"
            <device name="mini">
              <element_library>
                <element name="SW2">
                  <sram_info amount="1">
                    <sram name="EN" default="1"/>
                  </sram_info>
                  <path_info amount="1">
                    <path in="IN" out="OUT"/>
                  </path_info>
                </element>
              </element_library>
              <transmission_library>
                <signal_transmission name="CNTX" type="GSB_CNT"/>
              </transmission_library>
              <tile_library>
                <tile name="CENTER" sram_amount="R1C1">
                  <transmission_info amount="1">
                    <transmission type="CNTX">
                      <site name="GSB_CNT" position="R0C0">
                        <site_sram>
                          <sram basic_cell="sw0" sram_name="EN" local_place="B0W0"/>
                        </site_sram>
                      </site>
                    </transmission>
                  </transmission_info>
                </tile>
              </tile_library>
            </device>
            "##,
        )
        .expect("parse mini cil");
        let arch_xml = r#"
        <architecture>
          <library name="block">
            <cell name="GSB_CNT">
              <contents>
                <instance name="sw0" cellRef="SW2"/>
                <net name="IN">
                  <portRef instanceRef="sw0" name="IN"/>
                </net>
                <net name="OUT">
                  <portRef instanceRef="sw0" name="OUT"/>
                </net>
              </contents>
            </cell>
          </library>
        </architecture>
        "#;
        let file = NamedTempFile::new().expect("temp arch xml");
        fs::write(file.path(), arch_xml).expect("write arch xml");

        let defaults = load_site_route_defaults(file.path(), &cil).expect("load defaults");
        let bits = defaults.get("GSB_CNT").expect("GSB_CNT defaults");

        assert_eq!(bits.len(), 1);
        assert_eq!(bits[0].basic_cell, "sw0");
        assert_eq!(bits[0].sram_name, "EN");
        assert_eq!(bits[0].value, 1);
    }
}
