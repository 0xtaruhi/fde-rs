use crate::ir::{
    Cell, CellKind, CellPin, Cluster, ClusterKind, Design, Endpoint, EndpointKind, Net, Port,
    PortDirection, Property, RouteSegment, TimingPath, TimingPathCategory, TimingSummary,
};
use anyhow::{Context, Result, anyhow, bail};
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
            cell.kind.as_str(),
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
        let _ = writeln!(xml, "    </net>");
    }
    let _ = writeln!(xml, "  </nets>");

    let _ = writeln!(xml, "  <clusters>");
    for cluster in &design.clusters {
        let _ = writeln!(
            xml,
            "    <cluster name=\"{}\" kind=\"{}\" capacity=\"{}\" x=\"{}\" y=\"{}\" z=\"{}\" fixed=\"{}\">",
            escape(&cluster.name),
            cluster.kind.as_str(),
            cluster.capacity,
            cluster.x.map(|value| value.to_string()).unwrap_or_default(),
            cluster.y.map(|value| value.to_string()).unwrap_or_default(),
            cluster.z.map(|value| value.to_string()).unwrap_or_default(),
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
                path.category.as_str(),
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
        endpoint.kind.as_str(),
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

    let metadata_node = root
        .children()
        .find(|node| node.has_tag_name("metadata"))
        .ok_or_else(|| anyhow!("missing <metadata> section"))?;
    let notes = metadata_node
        .children()
        .filter(|node| node.has_tag_name("note"))
        .filter_map(|node| node.text())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    let mut design = Design {
        name: root.attribute("name").unwrap_or("design").to_string(),
        stage: root.attribute("stage").unwrap_or("unknown").to_string(),
        metadata: crate::ir::Metadata {
            source_format: metadata_node
                .attribute("source_format")
                .unwrap_or_default()
                .to_string(),
            family: metadata_node
                .attribute("family")
                .unwrap_or_default()
                .to_string(),
            arch_name: metadata_node
                .attribute("arch_name")
                .unwrap_or_default()
                .to_string(),
            lut_size: metadata_node
                .attribute("lut_size")
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
            let mut design_port = Port::new(
                attr(&port, "name"),
                attr(&port, "direction")
                    .parse()
                    .unwrap_or(PortDirection::Input),
            );
            design_port.width = attr(&port, "width").parse().unwrap_or(1);
            design_port.pin = non_empty_attr(&port, "pin");
            design_port.x = non_empty_attr(&port, "x").and_then(|value| value.parse().ok());
            design_port.y = non_empty_attr(&port, "y").and_then(|value| value.parse().ok());
            design.ports.push(design_port);
        }
    }

    if let Some(cells_node) = root.children().find(|node| node.has_tag_name("cells")) {
        for cell_node in cells_node
            .children()
            .filter(|node| node.has_tag_name("cell"))
        {
            let mut cell = Cell {
                name: attr(&cell_node, "name"),
                kind: attr(&cell_node, "kind")
                    .parse()
                    .unwrap_or(CellKind::Unknown),
                type_name: attr(&cell_node, "type_name"),
                cluster: non_empty_attr(&cell_node, "cluster"),
                ..Cell::default()
            };
            for child in cell_node.children().filter(|node| node.is_element()) {
                match child.tag_name().name() {
                    "property" => cell
                        .properties
                        .push(Property::new(attr(&child, "key"), attr(&child, "value"))),
                    "input" => cell
                        .inputs
                        .push(CellPin::new(attr(&child, "port"), attr(&child, "net"))),
                    "output" => cell
                        .outputs
                        .push(CellPin::new(attr(&child, "port"), attr(&child, "net"))),
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
                    "property" => net
                        .properties
                        .push(Property::new(attr(&child, "key"), attr(&child, "value"))),
                    "segment" => net.route.push(RouteSegment::new(
                        (
                            attr(&child, "x0").parse().unwrap_or(0),
                            attr(&child, "y0").parse().unwrap_or(0),
                        ),
                        (
                            attr(&child, "x1").parse().unwrap_or(0),
                            attr(&child, "y1").parse().unwrap_or(0),
                        ),
                    )),
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
            let mut cluster = Cluster::new(
                attr(&cluster_node, "name"),
                attr(&cluster_node, "kind")
                    .parse()
                    .unwrap_or(ClusterKind::Unknown),
            )
            .with_capacity(attr(&cluster_node, "capacity").parse().unwrap_or(0));
            cluster.x = non_empty_attr(&cluster_node, "x").and_then(|value| value.parse().ok());
            cluster.y = non_empty_attr(&cluster_node, "y").and_then(|value| value.parse().ok());
            cluster.z = non_empty_attr(&cluster_node, "z").and_then(|value| value.parse().ok());
            cluster.fixed = attr(&cluster_node, "fixed").parse().unwrap_or(false);
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
                category: attr(&path_node, "category")
                    .parse()
                    .unwrap_or(TimingPathCategory::Unknown),
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
        kind: attr(node, "kind").parse().unwrap_or(EndpointKind::Unknown),
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

#[cfg(test)]
mod tests {
    use super::{load_design_xml, save_design_xml};
    use crate::ir::{Cell, CellKind, Design, Endpoint, EndpointKind, Net};

    #[test]
    fn xml_roundtrip_preserves_typed_kinds() {
        let design = Design {
            name: "typed-kinds".to_string(),
            cells: vec![
                Cell::new("u0", CellKind::Lut, "LUT4")
                    .with_input("ADR0", "n0")
                    .with_output("O", "n1"),
            ],
            nets: vec![
                Net::new("n1")
                    .with_driver(Endpoint::cell("u0", "O"))
                    .with_sink(Endpoint::port("y", "OUT")),
            ],
            ..Design::default()
        };

        let xml = save_design_xml(&design);
        let loaded = load_design_xml(&xml).expect("xml roundtrip");

        assert_eq!(loaded.cells[0].kind, CellKind::Lut);
        assert_eq!(
            loaded.nets[0].driver.as_ref().map(|ep| ep.kind),
            Some(EndpointKind::Cell)
        );
        assert_eq!(loaded.nets[0].sinks[0].kind, EndpointKind::Port);
    }

    #[test]
    fn json_serde_keeps_string_shape_for_kinds() {
        let design = Design {
            name: "json-kinds".to_string(),
            cells: vec![Cell::new("u0", CellKind::Ff, "DFFHQ")],
            nets: vec![
                Net::new("n0")
                    .with_driver(Endpoint::cell("u0", "Q"))
                    .with_sink(Endpoint::port("y", "OUT")),
            ],
            ..Design::default()
        };

        let json = serde_json::to_string(&design).expect("json serialize");
        assert!(json.contains("\"kind\":\"ff\""));
        assert!(json.contains("\"kind\":\"cell\""));
        assert!(json.contains("\"kind\":\"port\""));
    }
}
