use crate::ir::Design;
use std::collections::BTreeMap;

pub(crate) fn annotate_net_criticality(design: &mut Design) {
    let forward = forward_levels(design);
    let backward = backward_levels(design);
    let max_forward = forward.values().copied().max().unwrap_or(1) as f64;
    let max_span = design
        .nets
        .iter()
        .map(|net| {
            forward.get(&net.name).copied().unwrap_or(0)
                + backward.get(&net.name).copied().unwrap_or(0)
        })
        .max()
        .unwrap_or(1) as f64;

    for net in &mut design.nets {
        let depth = forward.get(&net.name).copied().unwrap_or(0) as f64;
        let remaining = backward.get(&net.name).copied().unwrap_or(0) as f64;
        let span = depth + remaining;
        let fanout = net.sinks.len() as f64;
        let span_score = span / max_span.max(1.0);
        let depth_score = depth / max_forward.max(1.0);
        let fanout_score = (fanout / 8.0).min(1.0);
        net.criticality = 0.65 * span_score + 0.25 * depth_score + 0.10 * fanout_score;
    }
}

fn forward_levels(design: &Design) -> BTreeMap<String, usize> {
    let mut levels = BTreeMap::<String, usize>::new();
    for port in &design.ports {
        if port.direction.is_input_like() {
            levels.insert(port.name.clone(), 0);
        }
    }

    let mut changed = true;
    for _ in 0..design.cells.len().max(1) {
        if !changed {
            break;
        }
        changed = false;
        for cell in &design.cells {
            if cell.is_sequential() {
                for output in &cell.outputs {
                    if levels.insert(output.net.clone(), 0).is_none() {
                        changed = true;
                    }
                }
                continue;
            }

            let input_level = cell
                .inputs
                .iter()
                .filter_map(|pin| levels.get(&pin.net).copied())
                .max()
                .unwrap_or(0);
            for output in &cell.outputs {
                let candidate = input_level + 1;
                if candidate > *levels.get(&output.net).unwrap_or(&0) {
                    levels.insert(output.net.clone(), candidate);
                    changed = true;
                }
            }
        }
    }

    levels
}

fn backward_levels(design: &Design) -> BTreeMap<String, usize> {
    let mut levels = BTreeMap::<String, usize>::new();
    for net in &design.nets {
        if net.sinks.iter().any(|sink| {
            sink.is_port()
                && design
                    .port_lookup(&sink.name)
                    .is_some_and(|port| port.direction.is_output_like())
        }) {
            levels.insert(net.name.clone(), 0);
        }
    }
    for cell in &design.cells {
        if cell.is_sequential() {
            for input in &cell.inputs {
                levels.entry(input.net.clone()).or_insert(0);
            }
        }
    }

    let mut changed = true;
    for _ in 0..design.cells.len().max(1) {
        if !changed {
            break;
        }
        changed = false;
        for cell in design.cells.iter().rev() {
            if cell.is_sequential() {
                for input in &cell.inputs {
                    if levels.insert(input.net.clone(), 0).is_none() {
                        changed = true;
                    }
                }
                continue;
            }

            let output_level = cell
                .outputs
                .iter()
                .filter_map(|pin| levels.get(&pin.net).copied())
                .max()
                .unwrap_or(0);
            for input in &cell.inputs {
                let candidate = output_level + 1;
                if candidate > *levels.get(&input.net).unwrap_or(&0) {
                    levels.insert(input.net.clone(), candidate);
                    changed = true;
                }
            }
        }
    }

    levels
}

#[cfg(test)]
mod tests {
    use super::annotate_net_criticality;
    use crate::ir::{Cell, CellPin, Design, Endpoint, Net, Port, PortDirection};

    #[test]
    fn annotates_longer_path_as_more_critical() {
        let mut design = Design {
            ports: vec![
                Port {
                    name: "in".to_string(),
                    direction: PortDirection::Input,
                    ..Port::default()
                },
                Port {
                    name: "out".to_string(),
                    direction: PortDirection::Output,
                    ..Port::default()
                },
            ],
            cells: vec![
                Cell {
                    name: "u0".to_string(),
                    kind: "lut".to_string(),
                    type_name: "LUT4".to_string(),
                    inputs: vec![CellPin {
                        port: "A".to_string(),
                        net: "in_net".to_string(),
                    }],
                    outputs: vec![CellPin {
                        port: "O".to_string(),
                        net: "mid0".to_string(),
                    }],
                    ..Cell::default()
                },
                Cell {
                    name: "u1".to_string(),
                    kind: "lut".to_string(),
                    type_name: "LUT4".to_string(),
                    inputs: vec![CellPin {
                        port: "A".to_string(),
                        net: "mid0".to_string(),
                    }],
                    outputs: vec![CellPin {
                        port: "O".to_string(),
                        net: "mid1".to_string(),
                    }],
                    ..Cell::default()
                },
                Cell {
                    name: "u2".to_string(),
                    kind: "lut".to_string(),
                    type_name: "LUT4".to_string(),
                    inputs: vec![CellPin {
                        port: "A".to_string(),
                        net: "in_net".to_string(),
                    }],
                    outputs: vec![CellPin {
                        port: "O".to_string(),
                        net: "fast".to_string(),
                    }],
                    ..Cell::default()
                },
            ],
            nets: vec![
                Net {
                    name: "in_net".to_string(),
                    driver: Some(Endpoint {
                        kind: "port".to_string(),
                        name: "in".to_string(),
                        pin: "IN".to_string(),
                    }),
                    sinks: vec![
                        Endpoint {
                            kind: "cell".to_string(),
                            name: "u0".to_string(),
                            pin: "A".to_string(),
                        },
                        Endpoint {
                            kind: "cell".to_string(),
                            name: "u2".to_string(),
                            pin: "A".to_string(),
                        },
                    ],
                    ..Net::default()
                },
                Net {
                    name: "mid0".to_string(),
                    driver: Some(Endpoint {
                        kind: "cell".to_string(),
                        name: "u0".to_string(),
                        pin: "O".to_string(),
                    }),
                    sinks: vec![Endpoint {
                        kind: "cell".to_string(),
                        name: "u1".to_string(),
                        pin: "A".to_string(),
                    }],
                    ..Net::default()
                },
                Net {
                    name: "mid1".to_string(),
                    driver: Some(Endpoint {
                        kind: "cell".to_string(),
                        name: "u1".to_string(),
                        pin: "O".to_string(),
                    }),
                    sinks: vec![Endpoint {
                        kind: "port".to_string(),
                        name: "out".to_string(),
                        pin: "OUT".to_string(),
                    }],
                    ..Net::default()
                },
                Net {
                    name: "fast".to_string(),
                    driver: Some(Endpoint {
                        kind: "cell".to_string(),
                        name: "u2".to_string(),
                        pin: "O".to_string(),
                    }),
                    sinks: vec![Endpoint {
                        kind: "port".to_string(),
                        name: "out".to_string(),
                        pin: "OUT".to_string(),
                    }],
                    ..Net::default()
                },
            ],
            ..Design::default()
        };

        annotate_net_criticality(&mut design);

        let mid0 = design
            .nets
            .iter()
            .find(|net| net.name == "mid0")
            .map(|net| net.criticality)
            .unwrap_or(0.0);
        let fast = design
            .nets
            .iter()
            .find(|net| net.name == "fast")
            .map(|net| net.criticality)
            .unwrap_or(0.0);

        assert!(mid0 > fast);
    }
}
