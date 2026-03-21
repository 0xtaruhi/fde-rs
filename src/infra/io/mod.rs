use crate::ir::{
    Cell, CellPin, Cluster, Design, Endpoint, Net, Port, PortDirection, Property, RoutePip,
    RouteSegment, TimingPath, TimingSummary,
};
use anyhow::{Context, Result, bail};
use roxmltree::{Document, Node};
use std::{fmt::Write, fs, path::Path};

pub fn load_design(path: &Path) -> Result<Design> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read design {}", path.display()))?;
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "json" => serde_json::from_str(&text)
            .with_context(|| format!("failed to parse json design {}", path.display())),
        _ => load_design_xml(&text),
    }
}

pub fn save_design(design: &Design, path: &Path) -> Result<()> {
    let data = match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "json" => serde_json::to_string_pretty(design)?,
        _ => save_design_xml(design),
    };
    fs::write(path, data).with_context(|| format!("failed to write design {}", path.display()))
}

fn save_design_xml(design: &Design) -> String {
    let mut xml = String::new();
    let _ = writeln!(
        xml,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<design name=\"{}\" stage=\"{}\">",
        escape(&design.name),
        escape(&design.stage)
    );
    let _ = writeln!(
        xml,
        "  <metadata source_format=\"{}\" family=\"{}\" arch_name=\"{}\" lut_size=\"{}\">",
        escape(&design.metadata.source_format),
        escape(&design.metadata.family),
        escape(&design.metadata.arch_name),
        design.metadata.lut_size
    );
    for note in &design.metadata.notes {
        let _ = writeln!(xml, "    <note>{}</note>", escape(note));
    }
    let _ = writeln!(xml, "  </metadata>");

    let _ = writeln!(xml, "  <ports>");
    for port in &design.ports {
        let _ = writeln!(
            xml,
            "    <port name=\"{}\" direction=\"{}\" width=\"{}\" pin=\"{}\" x=\"{}\" y=\"{}\" />",
            escape(&port.name),
            port.direction.as_str(),
            port.width.max(1),
            escape(port.pin.as_deref().unwrap_or("")),
            port.x.map(|value| value.to_string()).unwrap_or_default(),
            port.y.map(|value| value.to_string()).unwrap_or_default(),
        );
    }
    let _ = writeln!(xml, "  </ports>");

    let _ = writeln!(xml, "  <cells>");
    for cell in &design.cells {
        let _ = writeln!(
            xml,
            "    <cell name=\"{}\" kind=\"{}\" type_name=\"{}\" cluster=\"{}\">",
            escape(&cell.name),
            escape(&cell.kind),
            escape(&cell.type_name),
            escape(cell.cluster.as_deref().unwrap_or(""))
        );
        for property in &cell.properties {
            let _ = writeln!(
                xml,
                "      <property key=\"{}\" value=\"{}\" />",
                escape(&property.key),
                escape(&property.value)
            );
        }
        for pin in &cell.inputs {
            let _ = writeln!(
                xml,
                "      <input port=\"{}\" net=\"{}\" />",
                escape(&pin.port),
                escape(&pin.net)
            );
        }
        for pin in &cell.outputs {
            let _ = writeln!(
                xml,
                "      <output port=\"{}\" net=\"{}\" />",
                escape(&pin.port),
                escape(&pin.net)
            );
        }
        let _ = writeln!(xml, "    </cell>");
    }
    let _ = writeln!(xml, "  </cells>");

    let _ = writeln!(xml, "  <nets>");
    for net in &design.nets {
        let _ = writeln!(
            xml,
            "    <net name=\"{}\" estimated_delay_ns=\"{:.6}\" criticality=\"{:.6}\">",
            escape(&net.name),
            net.estimated_delay_ns,
            net.criticality
        );
        if let Some(driver) = &net.driver {
            write_endpoint(&mut xml, "driver", driver, 6);
        }
        for sink in &net.sinks {
            write_endpoint(&mut xml, "sink", sink, 6);
        }
        for property in &net.properties {
            let _ = writeln!(
                xml,
                "      <property key=\"{}\" value=\"{}\" />",
                escape(&property.key),
                escape(&property.value)
            );
        }
        for segment in &net.route {
            let _ = writeln!(
                xml,
                "      <segment x0=\"{}\" y0=\"{}\" x1=\"{}\" y1=\"{}\" />",
                segment.x0, segment.y0, segment.x1, segment.y1
            );
        }
        for pip in &net.route_pips {
            let _ = writeln!(
                xml,
                "      <pip from=\"{}\" to=\"{}\" position=\"{}\" dir=\"{}\" />",
                escape(&pip.from_net),
                escape(&pip.to_net),
                format_point_position(pip.x, pip.y),
                escape(route_pip_dir(pip))
            );
        }
        let _ = writeln!(xml, "    </net>");
    }
    let _ = writeln!(xml, "  </nets>");

    let _ = writeln!(xml, "  <clusters>");
    for cluster in &design.clusters {
        let _ = writeln!(
            xml,
            "    <cluster name=\"{}\" kind=\"{}\" capacity=\"{}\" x=\"{}\" y=\"{}\" fixed=\"{}\">",
            escape(&cluster.name),
            escape(&cluster.kind),
            cluster.capacity,
            cluster.x.map(|value| value.to_string()).unwrap_or_default(),
            cluster.y.map(|value| value.to_string()).unwrap_or_default(),
            cluster.fixed
        );
        for member in &cluster.members {
            let _ = writeln!(xml, "      <member name=\"{}\" />", escape(member));
        }
        let _ = writeln!(xml, "    </cluster>");
    }
    let _ = writeln!(xml, "  </clusters>");

    if let Some(timing) = &design.timing {
        let _ = writeln!(
            xml,
            "  <timing critical_path_ns=\"{:.6}\" fmax_mhz=\"{:.6}\">",
            timing.critical_path_ns, timing.fmax_mhz
        );
        for path in &timing.top_paths {
            let _ = writeln!(
                xml,
                "    <path category=\"{}\" endpoint=\"{}\" delay_ns=\"{:.6}\">",
                escape(&path.category),
                escape(&path.endpoint),
                path.delay_ns
            );
            for hop in &path.hops {
                let _ = writeln!(xml, "      <hop name=\"{}\" />", escape(hop));
            }
            let _ = writeln!(xml, "    </path>");
        }
        let _ = writeln!(xml, "  </timing>");
    }

    let _ = writeln!(xml, "</design>");
    xml
}

fn write_endpoint(xml: &mut String, tag: &str, endpoint: &Endpoint, indent: usize) {
    let _ = writeln!(
        xml,
        "{space}<{tag} kind=\"{}\" name=\"{}\" pin=\"{}\" />",
        escape(&endpoint.kind),
        escape(&endpoint.name),
        escape(&endpoint.pin),
        space = " ".repeat(indent),
        tag = tag
    );
}

fn load_design_xml(xml: &str) -> Result<Design> {
    let doc = Document::parse(xml).context("failed to parse design xml")?;
    let root = doc.root_element();
    if !root.has_tag_name("design") {
        bail!("root element is not <design>");
    }

    let metadata_node = root.children().find(|node| node.has_tag_name("metadata"));
    let mut notes = metadata_node
        .into_iter()
        .flat_map(|node| node.children().filter(|child| child.has_tag_name("note")))
        .filter_map(|node| node.text())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if metadata_node.is_none() {
        notes.push("Loaded legacy design XML without a <metadata> section.".to_string());
    }

    let mut design = Design {
        name: root.attribute("name").unwrap_or("design").to_string(),
        stage: root.attribute("stage").unwrap_or("unknown").to_string(),
        metadata: crate::ir::Metadata {
            source_format: metadata_node
                .and_then(|node| node.attribute("source_format"))
                .unwrap_or_default()
                .to_string(),
            family: metadata_node
                .and_then(|node| node.attribute("family"))
                .unwrap_or_default()
                .to_string(),
            arch_name: metadata_node
                .and_then(|node| node.attribute("arch_name"))
                .unwrap_or_default()
                .to_string(),
            lut_size: metadata_node
                .and_then(|node| node.attribute("lut_size"))
                .unwrap_or("0")
                .parse()
                .unwrap_or(0),
            notes,
        },
        ..Design::default()
    };

    if let Some(ports_node) = root.children().find(|node| node.has_tag_name("ports")) {
        for port in ports_node
            .children()
            .filter(|node| node.has_tag_name("port"))
        {
            design.ports.push(Port {
                name: attr(&port, "name"),
                direction: attr(&port, "direction")
                    .parse()
                    .unwrap_or(PortDirection::Input),
                width: attr(&port, "width").parse().unwrap_or(1),
                pin: non_empty_attr(&port, "pin"),
                x: non_empty_attr(&port, "x").and_then(|value| value.parse().ok()),
                y: non_empty_attr(&port, "y").and_then(|value| value.parse().ok()),
            });
        }
    }

    if let Some(cells_node) = root.children().find(|node| node.has_tag_name("cells")) {
        for cell_node in cells_node
            .children()
            .filter(|node| node.has_tag_name("cell"))
        {
            let mut cell = Cell {
                name: attr(&cell_node, "name"),
                kind: attr(&cell_node, "kind"),
                type_name: attr(&cell_node, "type_name"),
                cluster: non_empty_attr(&cell_node, "cluster"),
                ..Cell::default()
            };
            for child in cell_node.children().filter(|node| node.is_element()) {
                match child.tag_name().name() {
                    "property" => cell.properties.push(Property {
                        key: attr(&child, "key"),
                        value: attr(&child, "value"),
                    }),
                    "input" => cell.inputs.push(CellPin {
                        port: attr(&child, "port"),
                        net: attr(&child, "net"),
                    }),
                    "output" => cell.outputs.push(CellPin {
                        port: attr(&child, "port"),
                        net: attr(&child, "net"),
                    }),
                    _ => {}
                }
            }
            design.cells.push(cell);
        }
    }

    if let Some(nets_node) = root.children().find(|node| node.has_tag_name("nets")) {
        for net_node in nets_node.children().filter(|node| node.has_tag_name("net")) {
            let mut net = Net {
                name: attr(&net_node, "name"),
                estimated_delay_ns: attr(&net_node, "estimated_delay_ns").parse().unwrap_or(0.0),
                criticality: attr(&net_node, "criticality").parse().unwrap_or(0.0),
                ..Net::default()
            };
            for child in net_node.children().filter(|node| node.is_element()) {
                match child.tag_name().name() {
                    "driver" => net.driver = Some(read_endpoint(&child)),
                    "sink" => net.sinks.push(read_endpoint(&child)),
                    "property" => net.properties.push(Property {
                        key: attr(&child, "key"),
                        value: attr(&child, "value"),
                    }),
                    "segment" => net.route.push(RouteSegment {
                        x0: attr(&child, "x0").parse().unwrap_or(0),
                        y0: attr(&child, "y0").parse().unwrap_or(0),
                        x1: attr(&child, "x1").parse().unwrap_or(0),
                        y1: attr(&child, "y1").parse().unwrap_or(0),
                    }),
                    "pip" => {
                        let (x, y) = parse_route_pip_position(&child).unwrap_or((0, 0));
                        net.route_pips.push(RoutePip {
                            x,
                            y,
                            from_net: attr(&child, "from"),
                            to_net: attr(&child, "to"),
                            dir: attr(&child, "dir"),
                        });
                    }
                    _ => {}
                }
            }
            design.nets.push(net);
        }
    }

    if let Some(clusters_node) = root.children().find(|node| node.has_tag_name("clusters")) {
        for cluster_node in clusters_node
            .children()
            .filter(|node| node.has_tag_name("cluster"))
        {
            let mut cluster = Cluster {
                name: attr(&cluster_node, "name"),
                kind: attr(&cluster_node, "kind"),
                capacity: attr(&cluster_node, "capacity").parse().unwrap_or(0),
                x: non_empty_attr(&cluster_node, "x").and_then(|value| value.parse().ok()),
                y: non_empty_attr(&cluster_node, "y").and_then(|value| value.parse().ok()),
                fixed: attr(&cluster_node, "fixed").parse().unwrap_or(false),
                ..Cluster::default()
            };
            for member in cluster_node
                .children()
                .filter(|node| node.has_tag_name("member"))
            {
                cluster.members.push(attr(&member, "name"));
            }
            design.clusters.push(cluster);
        }
    }

    if let Some(timing_node) = root.children().find(|node| node.has_tag_name("timing")) {
        let mut summary = TimingSummary {
            critical_path_ns: attr(&timing_node, "critical_path_ns")
                .parse()
                .unwrap_or(0.0),
            fmax_mhz: attr(&timing_node, "fmax_mhz").parse().unwrap_or(0.0),
            ..TimingSummary::default()
        };
        for path_node in timing_node
            .children()
            .filter(|node| node.has_tag_name("path"))
        {
            let mut path = TimingPath {
                category: attr(&path_node, "category"),
                endpoint: attr(&path_node, "endpoint"),
                delay_ns: attr(&path_node, "delay_ns").parse().unwrap_or(0.0),
                ..TimingPath::default()
            };
            for hop in path_node.children().filter(|node| node.has_tag_name("hop")) {
                path.hops.push(attr(&hop, "name"));
            }
            summary.top_paths.push(path);
        }
        design.timing = Some(summary);
    }

    Ok(design)
}

fn read_endpoint(node: &Node<'_, '_>) -> Endpoint {
    Endpoint {
        kind: attr(node, "kind"),
        name: attr(node, "name"),
        pin: attr(node, "pin"),
    }
}

fn attr(node: &Node<'_, '_>, name: &str) -> String {
    node.attribute(name).unwrap_or_default().to_string()
}

fn non_empty_attr(node: &Node<'_, '_>, name: &str) -> Option<String> {
    let value = attr(node, name);
    if value.is_empty() { None } else { Some(value) }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('\"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn format_point_position(x: usize, y: usize) -> String {
    format!("{x},{y}")
}

fn parse_route_pip_position(node: &Node<'_, '_>) -> Option<(usize, usize)> {
    if let Some(position) = node.attribute("position") {
        if let Some(raw) = position.strip_prefix('R') {
            let (row, col) = raw.split_once('C')?;
            return Some((col.parse().ok()?, row.parse().ok()?));
        }
        if let Some((x, y)) = position.split_once(',') {
            return Some((x.trim().parse().ok()?, y.trim().parse().ok()?));
        }
    }
    Some((
        node.attribute("x")?.parse().ok()?,
        node.attribute("y")?.parse().ok()?,
    ))
}

fn route_pip_dir(pip: &RoutePip) -> &str {
    match pip.dir.as_str() {
        "" | "unidir" => "->",
        value => value,
    }
}

#[cfg(test)]
mod tests {
    use super::{load_design_xml, save_design_xml};
    use crate::ir::{Design, Endpoint, Net, RoutePip};

    #[test]
    fn design_xml_roundtrip_preserves_exact_route_pips() {
        let design = Design {
            name: "exact-pip-roundtrip".to_string(),
            stage: "routed".to_string(),
            nets: vec![Net {
                name: "link".to_string(),
                driver: Some(Endpoint {
                    kind: "cell".to_string(),
                    name: "src".to_string(),
                    pin: "O".to_string(),
                }),
                sinks: vec![Endpoint {
                    kind: "cell".to_string(),
                    name: "dst".to_string(),
                    pin: "A".to_string(),
                }],
                route_pips: vec![RoutePip {
                    x: 3,
                    y: 4,
                    from_net: "SRC".to_string(),
                    to_net: "DST".to_string(),
                    dir: String::new(),
                }],
                ..Net::default()
            }],
            ..Design::default()
        };

        let xml = save_design_xml(&design);
        assert!(xml.contains("<pip from=\"SRC\" to=\"DST\" position=\"3,4\" dir=\"-&gt;\" />"));

        let loaded = load_design_xml(&xml).expect("reload exact-pip xml");
        assert_eq!(loaded.nets[0].route_pips.len(), 1);
        assert_eq!(loaded.nets[0].route_pips[0].x, 3);
        assert_eq!(loaded.nets[0].route_pips[0].y, 4);
        assert_eq!(loaded.nets[0].route_pips[0].from_net, "SRC");
        assert_eq!(loaded.nets[0].route_pips[0].to_net, "DST");
        assert_eq!(loaded.nets[0].route_pips[0].dir, "->");
    }

    #[test]
    fn design_xml_loads_cpp_style_pip_positions() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<design name="cpp-route" stage="routed">
  <metadata source_format="" family="" arch_name="" lut_size="0">
  </metadata>
  <ports>
  </ports>
  <cells>
  </cells>
  <nets>
    <net name="n" estimated_delay_ns="0.000000" criticality="0.000000">
      <pip from="A" to="B" position="2,5" dir="->" />
    </net>
  </nets>
  <clusters>
  </clusters>
</design>"#;
        let loaded = load_design_xml(xml).expect("load cpp style routed xml");
        assert_eq!(loaded.nets[0].route_pips.len(), 1);
        assert_eq!(loaded.nets[0].route_pips[0].x, 2);
        assert_eq!(loaded.nets[0].route_pips[0].y, 5);
        assert_eq!(loaded.nets[0].route_pips[0].dir, "->");
    }

    #[test]
    fn design_xml_loads_legacy_files_without_metadata() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<design name="legacy" stage="routed">
  <nets>
    <net name="n" estimated_delay_ns="0.000000" criticality="0.000000">
      <pip from="A" to="B" position="2,5" dir="->" />
    </net>
  </nets>
</design>"#;
        let loaded = load_design_xml(xml).expect("load legacy routed xml");
        assert_eq!(loaded.name, "legacy");
        assert_eq!(loaded.stage, "routed");
        assert_eq!(loaded.nets[0].route_pips.len(), 1);
        assert_eq!(
            loaded.metadata.notes,
            vec!["Loaded legacy design XML without a <metadata> section.".to_string()]
        );
    }
}
