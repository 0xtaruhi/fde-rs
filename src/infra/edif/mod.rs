use crate::{
    domain::{PinRole, PrimitiveKind},
    ir::{Cell, CellPin, Design, Endpoint, Net, Port, PortDirection},
};
use anyhow::{Context, Result, anyhow, bail};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone)]
enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

pub fn load_edif(path: &Path) -> Result<Design> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("failed to read EDIF file {}", path.display()))?;
    let tokens = tokenize(&source);
    let mut cursor = 0;
    let sexp = parse_sexp(&tokens, &mut cursor)?;
    build_design(&sexp)
}

fn tokenize(source: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars = source.chars().collect::<Vec<_>>();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '(' | ')' => {
                tokens.push(chars[i].to_string());
                i += 1;
            }
            '"' => {
                i += 1;
                let mut value = String::new();
                while i < chars.len() {
                    match chars[i] {
                        '"' => {
                            i += 1;
                            break;
                        }
                        '\\' if i + 1 < chars.len() => {
                            value.push(chars[i + 1]);
                            i += 2;
                        }
                        ch => {
                            value.push(ch);
                            i += 1;
                        }
                    }
                }
                tokens.push(value);
            }
            ';' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            ch if ch.is_whitespace() => i += 1,
            _ => {
                let start = i;
                while i < chars.len() {
                    let ch = chars[i];
                    if ch.is_whitespace() || ch == '(' || ch == ')' {
                        break;
                    }
                    i += 1;
                }
                tokens.push(chars[start..i].iter().collect());
            }
        }
    }
    tokens
}

fn parse_sexp(tokens: &[String], cursor: &mut usize) -> Result<Sexp> {
    let Some(token) = tokens.get(*cursor) else {
        bail!("unexpected end of EDIF input");
    };
    if token == "(" {
        *cursor += 1;
        let mut children = Vec::new();
        while tokens.get(*cursor).map(|token| token.as_str()) != Some(")") {
            children.push(parse_sexp(tokens, cursor)?);
            if *cursor >= tokens.len() {
                bail!("unterminated EDIF list");
            }
        }
        *cursor += 1;
        Ok(Sexp::List(children))
    } else if token == ")" {
        bail!("unexpected ')' in EDIF input")
    } else {
        *cursor += 1;
        Ok(Sexp::Atom(token.clone()))
    }
}

fn build_design(root: &Sexp) -> Result<Design> {
    let root_list = as_list(root)?;
    expect_head(root_list, "edif")?;
    let top_name = root_list.get(1).and_then(atom).unwrap_or("design");

    let design_library = root_list.iter().find_map(|item| {
        let list = as_list(item).ok()?;
        if list.first().and_then(atom) == Some("library")
            && list.get(1).and_then(atom) == Some("DESIGN")
        {
            Some(list)
        } else {
            None
        }
    });
    let library = design_library.ok_or_else(|| anyhow!("missing DESIGN library in EDIF"))?;
    let top_cell = library.iter().find_map(|item| {
        let list = as_list(item).ok()?;
        if list.first().and_then(atom) == Some("cell")
            && resolve_name(list.get(1)) == Some(top_name.to_string())
        {
            Some(list)
        } else {
            None
        }
    });
    let cell = top_cell.ok_or_else(|| anyhow!("missing top cell {top_name} in EDIF"))?;
    let view = find_named_child(cell, "view").ok_or_else(|| anyhow!("missing top view"))?;
    let interface =
        find_named_child(view, "interface").ok_or_else(|| anyhow!("missing top interface"))?;
    let contents =
        find_named_child(view, "contents").ok_or_else(|| anyhow!("missing top contents"))?;

    let mut design = Design {
        name: top_name.to_string(),
        stage: "mapped".to_string(),
        ..Design::default()
    };
    design.metadata.source_format = "edif".to_string();

    for port_sexp in interface.iter().filter(|item| has_head(item, "port")) {
        let list = as_list(port_sexp)?;
        let name = resolve_name(list.get(1)).ok_or_else(|| anyhow!("malformed port"))?;
        let direction = find_named_child(list, "direction")
            .and_then(|node| node.get(1))
            .and_then(atom)
            .unwrap_or("INPUT")
            .parse()
            .unwrap_or(PortDirection::Input);
        design.ports.push(Port {
            name,
            direction,
            width: 1,
            ..Port::default()
        });
    }

    let mut cell_types: BTreeMap<String, String> = BTreeMap::new();
    let mut instance_names: BTreeMap<String, String> = BTreeMap::new();
    for instance_sexp in contents.iter().filter(|item| has_head(item, "instance")) {
        let instance = as_list(instance_sexp)?;
        let instance_ref = resolve_reference_name(instance.get(1))
            .ok_or_else(|| anyhow!("malformed instance reference"))?;
        let name = resolve_name(instance.get(1)).unwrap_or_else(|| instance_ref.clone());
        let view_ref =
            find_named_child(instance, "viewRef").ok_or_else(|| anyhow!("missing viewRef"))?;
        let cell_ref =
            find_named_child(view_ref, "cellRef").ok_or_else(|| anyhow!("missing cellRef"))?;
        let type_name = resolve_name(cell_ref.get(1)).unwrap_or_else(|| "GENERIC".to_string());
        instance_names.insert(instance_ref, name.clone());
        cell_types.insert(name.clone(), type_name.clone());
        let mut cell = Cell {
            name,
            kind: classify_cell_kind(&type_name),
            type_name,
            ..Cell::default()
        };
        for property in instance.iter().filter(|item| has_head(item, "property")) {
            let property = as_list(property)?;
            let key = resolve_name(property.get(1)).unwrap_or_default();
            let value = property
                .iter()
                .skip(2)
                .find_map(read_value)
                .unwrap_or_default();
            if !key.is_empty() {
                cell.set_property(key.to_ascii_lowercase(), value);
            }
        }
        design.cells.push(cell);
    }

    for net_sexp in contents.iter().filter(|item| has_head(item, "net")) {
        let list = as_list(net_sexp)?;
        let name = resolve_name(list.get(1)).ok_or_else(|| anyhow!("malformed net"))?;
        let joined = find_named_child(list, "joined").ok_or_else(|| anyhow!("missing joined"))?;
        let mut endpoints = Vec::new();
        for port_ref in joined.iter().filter(|item| has_head(item, "portRef")) {
            let port_ref = as_list(port_ref)?;
            let pin = resolve_name(port_ref.get(1)).unwrap_or_default();
            let instance_ref = find_named_child(port_ref, "instanceRef")
                .and_then(|node| resolve_reference_name(node.get(1)));
            if let Some(instance_name) = instance_ref {
                let instance_name = instance_names
                    .get(&instance_name)
                    .cloned()
                    .unwrap_or(instance_name);
                endpoints.push(Endpoint {
                    kind: "cell".to_string(),
                    name: instance_name,
                    pin,
                });
            } else {
                endpoints.push(Endpoint {
                    kind: "port".to_string(),
                    name: pin.clone(),
                    pin,
                });
            }
        }
        let (driver, sinks) = split_endpoints(&design, &cell_types, &endpoints);
        design.nets.push(Net {
            name: name.clone(),
            driver,
            sinks,
            ..Net::default()
        });
    }

    for cell in &mut design.cells {
        cell.inputs.clear();
        cell.outputs.clear();
    }
    let cell_index = design
        .cells
        .iter()
        .enumerate()
        .map(|(index, cell)| (cell.name.clone(), index))
        .collect::<BTreeMap<_, _>>();
    for net in &design.nets {
        if let Some(driver) = &net.driver
            && driver.kind == "cell"
            && let Some(index) = cell_index.get(&driver.name)
        {
            design.cells[*index].outputs.push(CellPin {
                port: driver.pin.clone(),
                net: net.name.clone(),
            });
        }
        for sink in &net.sinks {
            if sink.kind == "cell"
                && let Some(index) = cell_index.get(&sink.name)
            {
                design.cells[*index].inputs.push(CellPin {
                    port: sink.pin.clone(),
                    net: net.name.clone(),
                });
            }
        }
    }

    Ok(design)
}

fn split_endpoints(
    design: &Design,
    cell_types: &BTreeMap<String, String>,
    endpoints: &[Endpoint],
) -> (Option<Endpoint>, Vec<Endpoint>) {
    let port_dirs = design
        .ports
        .iter()
        .map(|port| (port.name.clone(), port.direction.clone()))
        .collect::<BTreeMap<_, _>>();

    let mut sources = Vec::new();
    let mut sinks = Vec::new();
    for endpoint in endpoints {
        let is_source = match endpoint.kind.as_str() {
            "port" => port_dirs
                .get(&endpoint.name)
                .map(PortDirection::is_input_like)
                .unwrap_or(false),
            "cell" => cell_types
                .get(&endpoint.name)
                .map(|type_name| is_output_pin(type_name, &endpoint.pin))
                .unwrap_or(false),
            _ => false,
        };
        if is_source {
            sources.push(endpoint.clone());
        } else {
            sinks.push(endpoint.clone());
        }
    }

    let driver = sources
        .iter()
        .find(|endpoint| endpoint.kind == "cell")
        .cloned()
        .or_else(|| sources.first().cloned())
        .or_else(|| sinks.first().cloned());
    let sinks = endpoints
        .iter()
        .filter(|endpoint| Some(endpoint.key()) != driver.as_ref().map(Endpoint::key))
        .cloned()
        .collect::<Vec<_>>();
    (driver, sinks)
}

fn classify_cell_kind(type_name: &str) -> String {
    match PrimitiveKind::classify("", type_name) {
        PrimitiveKind::Lut { .. } => "lut".to_string(),
        PrimitiveKind::FlipFlop | PrimitiveKind::Latch => "ff".to_string(),
        PrimitiveKind::Constant(_) => "constant".to_string(),
        PrimitiveKind::Buffer => "buffer".to_string(),
        PrimitiveKind::Generic
        | PrimitiveKind::Unknown
        | PrimitiveKind::Io
        | PrimitiveKind::GlobalClockBuffer => "generic".to_string(),
    }
}

fn is_output_pin(type_name: &str, pin: &str) -> bool {
    PinRole::classify_output_pin(PrimitiveKind::classify("", type_name), pin).is_output_like()
}

fn has_head(sexp: &Sexp, head: &str) -> bool {
    as_list(sexp)
        .ok()
        .and_then(|list| list.first())
        .and_then(atom)
        == Some(head)
}

fn find_named_child<'a>(list: &'a [Sexp], head: &str) -> Option<&'a [Sexp]> {
    list.iter().find_map(|item| {
        let list = as_list(item).ok()?;
        if list.first().and_then(atom) == Some(head) {
            Some(list)
        } else {
            None
        }
    })
}

fn expect_head(list: &[Sexp], head: &str) -> Result<()> {
    if list.first().and_then(atom) == Some(head) {
        Ok(())
    } else {
        Err(anyhow!("expected head {head}"))
    }
}

fn resolve_name(sexp: Option<&Sexp>) -> Option<String> {
    let sexp = sexp?;
    match sexp {
        Sexp::Atom(value) => Some(value.clone()),
        Sexp::List(list) => match list.first().and_then(atom) {
            Some("rename") => list
                .get(2)
                .and_then(atom)
                .map(ToString::to_string)
                .or_else(|| list.get(1).and_then(atom).map(ToString::to_string)),
            Some("array") => list.get(1).and_then(atom).map(ToString::to_string),
            _ => None,
        },
    }
}

fn resolve_reference_name(sexp: Option<&Sexp>) -> Option<String> {
    let sexp = sexp?;
    match sexp {
        Sexp::Atom(value) => Some(value.clone()),
        Sexp::List(list) => match list.first().and_then(atom) {
            Some("rename") | Some("array") => list.get(1).and_then(atom).map(ToString::to_string),
            _ => resolve_name(Some(sexp)),
        },
    }
}

fn read_value(sexp: &Sexp) -> Option<String> {
    let list = as_list(sexp).ok()?;
    match list.first().and_then(atom) {
        Some("integer") | Some("string") => list.get(1).and_then(atom).map(ToString::to_string),
        _ => None,
    }
}

fn as_list(sexp: &Sexp) -> Result<&[Sexp]> {
    match sexp {
        Sexp::List(list) => Ok(list),
        _ => bail!("expected list"),
    }
}

fn atom(sexp: &Sexp) -> Option<&str> {
    match sexp {
        Sexp::Atom(value) => Some(value.as_str()),
        _ => None,
    }
}
