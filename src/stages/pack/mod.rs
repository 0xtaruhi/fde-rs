use crate::{
    ir::{Cluster, Design, Net},
    report::{StageOutput, StageReport},
};
use anyhow::Result;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

#[derive(Debug, Clone)]
pub struct PackOptions {
    pub family: Option<String>,
    pub capacity: usize,
    pub cell_library: Option<PathBuf>,
    pub dcp_library: Option<PathBuf>,
    pub config: Option<PathBuf>,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self {
            family: None,
            capacity: 4,
            cell_library: None,
            dcp_library: None,
            config: None,
        }
    }
}

pub fn run(mut design: Design, options: &PackOptions) -> Result<StageOutput<Design>> {
    let capacity = options.capacity.max(2);
    design.stage = "packed".to_string();
    if let Some(family) = &options.family {
        design.metadata.family = family.clone();
    }
    if let Some(cell_library) = &options.cell_library {
        design.note(format!(
            "Pack referenced cell library {}",
            cell_library.display()
        ));
    }
    if let Some(dcp_library) = &options.dcp_library {
        design.note(format!(
            "Pack referenced dc library {}",
            dcp_library.display()
        ));
    }
    if let Some(config) = &options.config {
        design.note(format!("Pack referenced config {}", config.display()));
    }

    let mut used = BTreeSet::new();
    let mut clusters = Vec::<Cluster>::new();
    let net_drivers = net_driver_cells(&design);
    let net_to_sinks = net_sink_cells(&design);

    for cell in design.cells.iter().filter(|cell| cell.is_sequential()) {
        if used.contains(&cell.name) {
            continue;
        }
        let d_net = cell
            .inputs
            .iter()
            .find(|pin| pin.port.eq_ignore_ascii_case("D"))
            .map(|pin| pin.net.clone());
        let mut members = Vec::new();
        if let Some(d_net) = d_net.as_ref()
            && let Some(driver) = net_drivers.get(d_net)
            && !used.contains(driver)
        {
            members.push(driver.clone());
            used.insert(driver.clone());
        }
        members.push(cell.name.clone());
        used.insert(cell.name.clone());
        extend_cluster_with_neighbors(&design, &mut members, &mut used, capacity);
        clusters.push(Cluster {
            name: next_cluster_name(clusters.len()),
            kind: "logic".to_string(),
            members,
            capacity,
            ..Cluster::default()
        });
    }

    let mut degree = design
        .cells
        .iter()
        .filter(|cell| !cell.is_constant_source())
        .map(|cell| {
            let fanout = cell
                .outputs
                .iter()
                .map(|pin| net_to_sinks.get(&pin.net).map(Vec::len).unwrap_or(0))
                .sum::<usize>();
            (cell.name.clone(), fanout + cell.inputs.len())
        })
        .collect::<Vec<_>>();
    degree.sort_by(|lhs, rhs| rhs.1.cmp(&lhs.1).then_with(|| lhs.0.cmp(&rhs.0)));

    for (cell_name, _) in degree {
        if used.contains(&cell_name) {
            continue;
        }
        let mut members = vec![cell_name.clone()];
        used.insert(cell_name.clone());
        extend_cluster_with_neighbors(&design, &mut members, &mut used, capacity);
        clusters.push(Cluster {
            name: next_cluster_name(clusters.len()),
            kind: "logic".to_string(),
            members,
            capacity,
            ..Cluster::default()
        });
    }

    for cluster in &clusters {
        for member in &cluster.members {
            if let Some(cell) = design.cells.iter_mut().find(|cell| &cell.name == member) {
                cell.cluster = Some(cluster.name.clone());
            }
        }
    }

    design.clusters = clusters;
    let mut report = StageReport::new("pack");
    report.push(format!(
        "Packed {} logical cells into {} clusters (capacity {}).",
        design.cells.len(),
        design.clusters.len(),
        capacity
    ));

    Ok(StageOutput {
        value: design,
        report,
    })
}

fn next_cluster_name(index: usize) -> String {
    format!("clb_{index:04}")
}

fn extend_cluster_with_neighbors(
    design: &Design,
    members: &mut Vec<String>,
    used: &mut BTreeSet<String>,
    capacity: usize,
) {
    while members.len() < capacity {
        let mut candidates = BTreeSet::new();
        for member in members.iter() {
            candidates.extend(neighbors_of_cell(design, member));
        }
        let remaining = capacity - members.len();
        let mut added_any = false;
        for candidate in candidates {
            if used.contains(&candidate) {
                continue;
            }
            let mut additions = vec![candidate.clone()];
            if remaining > 1
                && let Some(companion) = paired_pack_neighbor(design, &candidate)
                && !used.contains(&companion)
                && !members.iter().any(|member| member == &companion)
            {
                additions.push(companion);
            }
            for addition in additions.into_iter().take(remaining) {
                if used.insert(addition.clone()) {
                    members.push(addition);
                    added_any = true;
                }
            }
            break;
        }
        if !added_any {
            break;
        }
    }
}

fn paired_pack_neighbor(design: &Design, cell_name: &str) -> Option<String> {
    let cell = design.cells.iter().find(|cell| cell.name == cell_name)?;
    if cell.is_sequential() {
        let d_net = cell
            .inputs
            .iter()
            .find(|pin| pin.port.eq_ignore_ascii_case("D"))?
            .net
            .clone();
        return design
            .nets
            .iter()
            .find(|net| net.name == d_net)
            .and_then(|net| net.driver.as_ref())
            .filter(|driver| driver.kind == "cell")
            .map(|driver| driver.name.clone());
    }

    let mut sequential_sinks = cell
        .outputs
        .iter()
        .flat_map(|pin| {
            design
                .nets
                .iter()
                .find(|net| net.name == pin.net)
                .into_iter()
                .flat_map(|net| net.sinks.iter())
        })
        .filter(|sink| sink.kind == "cell")
        .filter_map(|sink| {
            design
                .cells
                .iter()
                .find(|cell| cell.name == sink.name)
                .filter(|cell| cell.is_sequential())
                .map(|_| sink.name.clone())
        })
        .collect::<Vec<_>>();
    sequential_sinks.sort();
    sequential_sinks.dedup();
    sequential_sinks.into_iter().next()
}

fn net_driver_cells(design: &Design) -> BTreeMap<String, String> {
    design
        .nets
        .iter()
        .filter_map(|net| {
            let driver = net.driver.as_ref()?;
            (driver.kind == "cell").then(|| (net.name.clone(), driver.name.clone()))
        })
        .collect()
}

fn net_sink_cells(design: &Design) -> BTreeMap<String, Vec<String>> {
    let mut map = BTreeMap::<String, Vec<String>>::new();
    for net in &design.nets {
        let sinks = net
            .sinks
            .iter()
            .filter(|sink| sink.kind == "cell")
            .map(|sink| sink.name.clone())
            .collect::<Vec<_>>();
        map.insert(net.name.clone(), sinks);
    }
    map
}

fn neighbors_of_cell(design: &Design, cell_name: &str) -> BTreeSet<String> {
    let mut neighbors = BTreeSet::new();
    for net in &design.nets {
        if !net_forms_pack_adjacency(design, net) {
            continue;
        }
        let touches_driver = net
            .driver
            .as_ref()
            .is_some_and(|driver| driver.kind == "cell" && driver.name == cell_name);
        let touches_sink = net
            .sinks
            .iter()
            .any(|sink| sink.kind == "cell" && sink.name == cell_name);
        if touches_driver || touches_sink {
            if let Some(driver) = &net.driver
                && driver.kind == "cell"
                && driver.name != cell_name
            {
                neighbors.insert(driver.name.clone());
            }
            for sink in &net.sinks {
                if sink.kind == "cell" && sink.name != cell_name {
                    neighbors.insert(sink.name.clone());
                }
            }
        }
    }
    neighbors
}

fn net_forms_pack_adjacency(design: &Design, net: &Net) -> bool {
    let Some(driver) = net.driver.as_ref() else {
        return false;
    };
    if driver.kind != "cell" {
        return false;
    }
    if design
        .cell_lookup(&driver.name)
        .is_some_and(|cell| cell.is_constant_source())
    {
        return false;
    }
    net.sinks.iter().any(|sink| sink.kind == "cell")
}

#[cfg(test)]
mod tests {
    use super::{PackOptions, run};
    use crate::ir::{Cell, CellPin, Design, Endpoint, Net};
    use anyhow::Result;
    use std::collections::BTreeSet;

    fn pack_design() -> Design {
        Design {
            name: "pack-mini".to_string(),
            cells: vec![
                Cell {
                    name: "lut_ff_driver".to_string(),
                    kind: "lut".to_string(),
                    type_name: "LUT4".to_string(),
                    outputs: vec![CellPin {
                        port: "O".to_string(),
                        net: "d_net".to_string(),
                    }],
                    ..Cell::default()
                },
                Cell {
                    name: "reg0".to_string(),
                    kind: "ff".to_string(),
                    type_name: "DFFHQ".to_string(),
                    inputs: vec![CellPin {
                        port: "D".to_string(),
                        net: "d_net".to_string(),
                    }],
                    outputs: vec![CellPin {
                        port: "Q".to_string(),
                        net: "q_net".to_string(),
                    }],
                    ..Cell::default()
                },
                Cell {
                    name: "lut_a".to_string(),
                    kind: "lut".to_string(),
                    type_name: "LUT4".to_string(),
                    inputs: vec![CellPin {
                        port: "A".to_string(),
                        net: "q_net".to_string(),
                    }],
                    outputs: vec![CellPin {
                        port: "O".to_string(),
                        net: "fanout".to_string(),
                    }],
                    ..Cell::default()
                },
                Cell {
                    name: "lut_b".to_string(),
                    kind: "lut".to_string(),
                    type_name: "LUT4".to_string(),
                    inputs: vec![CellPin {
                        port: "A".to_string(),
                        net: "fanout".to_string(),
                    }],
                    ..Cell::default()
                },
            ],
            nets: vec![
                Net {
                    name: "d_net".to_string(),
                    driver: Some(Endpoint {
                        kind: "cell".to_string(),
                        name: "lut_ff_driver".to_string(),
                        pin: "O".to_string(),
                    }),
                    sinks: vec![Endpoint {
                        kind: "cell".to_string(),
                        name: "reg0".to_string(),
                        pin: "D".to_string(),
                    }],
                    ..Net::default()
                },
                Net {
                    name: "q_net".to_string(),
                    driver: Some(Endpoint {
                        kind: "cell".to_string(),
                        name: "reg0".to_string(),
                        pin: "Q".to_string(),
                    }),
                    sinks: vec![Endpoint {
                        kind: "cell".to_string(),
                        name: "lut_a".to_string(),
                        pin: "A".to_string(),
                    }],
                    ..Net::default()
                },
                Net {
                    name: "fanout".to_string(),
                    driver: Some(Endpoint {
                        kind: "cell".to_string(),
                        name: "lut_a".to_string(),
                        pin: "O".to_string(),
                    }),
                    sinks: vec![Endpoint {
                        kind: "cell".to_string(),
                        name: "lut_b".to_string(),
                        pin: "A".to_string(),
                    }],
                    ..Net::default()
                },
            ],
            ..Design::default()
        }
    }

    fn pack_chain_design() -> Design {
        Design {
            name: "pack-chain".to_string(),
            cells: vec![
                Cell {
                    name: "lut0".to_string(),
                    kind: "lut".to_string(),
                    type_name: "LUT4".to_string(),
                    outputs: vec![CellPin {
                        port: "O".to_string(),
                        net: "d0".to_string(),
                    }],
                    ..Cell::default()
                },
                Cell {
                    name: "ff0".to_string(),
                    kind: "ff".to_string(),
                    type_name: "DFFHQ".to_string(),
                    inputs: vec![CellPin {
                        port: "D".to_string(),
                        net: "d0".to_string(),
                    }],
                    outputs: vec![CellPin {
                        port: "Q".to_string(),
                        net: "q0".to_string(),
                    }],
                    ..Cell::default()
                },
                Cell {
                    name: "lut1".to_string(),
                    kind: "lut".to_string(),
                    type_name: "LUT4".to_string(),
                    inputs: vec![CellPin {
                        port: "A".to_string(),
                        net: "q0".to_string(),
                    }],
                    outputs: vec![CellPin {
                        port: "O".to_string(),
                        net: "d1".to_string(),
                    }],
                    ..Cell::default()
                },
                Cell {
                    name: "ff1".to_string(),
                    kind: "ff".to_string(),
                    type_name: "DFFHQ".to_string(),
                    inputs: vec![CellPin {
                        port: "D".to_string(),
                        net: "d1".to_string(),
                    }],
                    ..Cell::default()
                },
            ],
            nets: vec![
                Net {
                    name: "d0".to_string(),
                    driver: Some(Endpoint {
                        kind: "cell".to_string(),
                        name: "lut0".to_string(),
                        pin: "O".to_string(),
                    }),
                    sinks: vec![Endpoint {
                        kind: "cell".to_string(),
                        name: "ff0".to_string(),
                        pin: "D".to_string(),
                    }],
                    ..Net::default()
                },
                Net {
                    name: "q0".to_string(),
                    driver: Some(Endpoint {
                        kind: "cell".to_string(),
                        name: "ff0".to_string(),
                        pin: "Q".to_string(),
                    }),
                    sinks: vec![Endpoint {
                        kind: "cell".to_string(),
                        name: "lut1".to_string(),
                        pin: "A".to_string(),
                    }],
                    ..Net::default()
                },
                Net {
                    name: "d1".to_string(),
                    driver: Some(Endpoint {
                        kind: "cell".to_string(),
                        name: "lut1".to_string(),
                        pin: "O".to_string(),
                    }),
                    sinks: vec![Endpoint {
                        kind: "cell".to_string(),
                        name: "ff1".to_string(),
                        pin: "D".to_string(),
                    }],
                    ..Net::default()
                },
            ],
            ..Design::default()
        }
    }

    fn pack_shared_clock_shift_design() -> Design {
        let mut design = Design {
            name: "pack-shared-clock-shift".to_string(),
            ..Design::default()
        };

        for index in 0..4 {
            design.cells.push(Cell {
                name: format!("ff{index}"),
                kind: "ff".to_string(),
                type_name: "DFFHQ".to_string(),
                inputs: vec![
                    CellPin {
                        port: "CK".to_string(),
                        net: "clk".to_string(),
                    },
                    CellPin {
                        port: "D".to_string(),
                        net: format!("d{index}"),
                    },
                ],
                outputs: vec![CellPin {
                    port: "Q".to_string(),
                    net: format!("q{index}"),
                }],
                ..Cell::default()
            });
            design.cells.push(Cell {
                name: format!("lut{index}"),
                kind: "lut".to_string(),
                type_name: "LUT2".to_string(),
                inputs: vec![
                    CellPin {
                        port: "ADR1".to_string(),
                        net: "clk".to_string(),
                    },
                    CellPin {
                        port: "ADR0".to_string(),
                        net: if index == 0 {
                            "din".to_string()
                        } else {
                            format!("q{}", index - 1)
                        },
                    },
                ],
                outputs: vec![CellPin {
                    port: "O".to_string(),
                    net: format!("d{index}"),
                }],
                ..Cell::default()
            });
        }

        design.nets.push(Net {
            name: "clk".to_string(),
            driver: Some(Endpoint {
                kind: "port".to_string(),
                name: "clk".to_string(),
                pin: "clk".to_string(),
            }),
            sinks: (0..4)
                .flat_map(|index| {
                    [
                        Endpoint {
                            kind: "cell".to_string(),
                            name: format!("lut{index}"),
                            pin: "ADR1".to_string(),
                        },
                        Endpoint {
                            kind: "cell".to_string(),
                            name: format!("ff{index}"),
                            pin: "CK".to_string(),
                        },
                    ]
                })
                .collect(),
            ..Net::default()
        });
        design.nets.push(Net {
            name: "din".to_string(),
            driver: Some(Endpoint {
                kind: "port".to_string(),
                name: "din".to_string(),
                pin: "din".to_string(),
            }),
            sinks: vec![Endpoint {
                kind: "cell".to_string(),
                name: "lut0".to_string(),
                pin: "ADR0".to_string(),
            }],
            ..Net::default()
        });

        for index in 0..4 {
            design.nets.push(Net {
                name: format!("d{index}"),
                driver: Some(Endpoint {
                    kind: "cell".to_string(),
                    name: format!("lut{index}"),
                    pin: "O".to_string(),
                }),
                sinks: vec![Endpoint {
                    kind: "cell".to_string(),
                    name: format!("ff{index}"),
                    pin: "D".to_string(),
                }],
                ..Net::default()
            });
        }

        for index in 0..4 {
            let mut sinks = Vec::new();
            if index < 3 {
                sinks.push(Endpoint {
                    kind: "cell".to_string(),
                    name: format!("lut{}", index + 1),
                    pin: "ADR0".to_string(),
                });
            }
            if index > 0 {
                sinks.push(Endpoint {
                    kind: "port".to_string(),
                    name: format!("q{index}"),
                    pin: format!("q{index}"),
                });
            }
            design.nets.push(Net {
                name: format!("q{index}"),
                driver: Some(Endpoint {
                    kind: "cell".to_string(),
                    name: format!("ff{index}"),
                    pin: "Q".to_string(),
                }),
                sinks,
                ..Net::default()
            });
        }

        design
    }

    #[test]
    fn pack_pairs_lut_with_sequential_d_input_and_respects_capacity() -> Result<()> {
        let packed = run(
            pack_design(),
            &PackOptions {
                family: Some("fdp3".to_string()),
                capacity: 2,
                ..PackOptions::default()
            },
        )?
        .value;

        assert_eq!(packed.stage, "packed");
        assert_eq!(packed.metadata.family, "fdp3");
        assert_eq!(packed.clusters.len(), 2);
        assert!(
            packed
                .clusters
                .iter()
                .all(|cluster| cluster.members.len() <= 2)
        );

        let ff_cluster = packed
            .clusters
            .iter()
            .find(|cluster| cluster.members.iter().any(|member| member == "reg0"))
            .expect("cluster containing reg0");
        assert!(
            ff_cluster
                .members
                .iter()
                .any(|member| member == "lut_ff_driver")
        );

        let remaining_cluster = packed
            .clusters
            .iter()
            .find(|cluster| cluster.name != ff_cluster.name)
            .expect("remaining cluster");
        assert_eq!(
            remaining_cluster.members,
            vec!["lut_a".to_string(), "lut_b".to_string()]
        );

        for cell in &packed.cells {
            assert!(
                cell.cluster.is_some(),
                "expected packed cluster for {}",
                cell.name
            );
        }

        Ok(())
    }

    #[test]
    fn pack_scales_across_multiple_independent_lut_ff_pairs() -> Result<()> {
        let mut design = Design {
            name: "pack-many".to_string(),
            ..Design::default()
        };
        for index in 0..8 {
            let lut_name = format!("lut_{index}");
            let ff_name = format!("ff_{index}");
            let net_name = format!("d_net_{index}");
            design.cells.push(Cell {
                name: lut_name.clone(),
                kind: "lut".to_string(),
                type_name: "LUT4".to_string(),
                outputs: vec![CellPin {
                    port: "O".to_string(),
                    net: net_name.clone(),
                }],
                ..Cell::default()
            });
            design.cells.push(Cell {
                name: ff_name.clone(),
                kind: "ff".to_string(),
                type_name: "DFFHQ".to_string(),
                inputs: vec![CellPin {
                    port: "D".to_string(),
                    net: net_name.clone(),
                }],
                ..Cell::default()
            });
            design.nets.push(Net {
                name: net_name,
                driver: Some(Endpoint {
                    kind: "cell".to_string(),
                    name: lut_name,
                    pin: "O".to_string(),
                }),
                sinks: vec![Endpoint {
                    kind: "cell".to_string(),
                    name: ff_name,
                    pin: "D".to_string(),
                }],
                ..Net::default()
            });
        }

        let packed = run(
            design,
            &PackOptions {
                capacity: 2,
                ..PackOptions::default()
            },
        )?
        .value;

        assert_eq!(packed.clusters.len(), 8);
        for index in 0..8 {
            let lut_name = format!("lut_{index}");
            let ff_name = format!("ff_{index}");
            let cluster = packed
                .clusters
                .iter()
                .find(|cluster| cluster.members.iter().any(|member| member == &ff_name))
                .expect("matching ff cluster");
            assert_eq!(cluster.members.len(), 2);
            assert!(cluster.members.iter().any(|member| member == &lut_name));
        }

        Ok(())
    }

    #[test]
    fn pack_fills_larger_clusters_with_connected_lut_ff_neighbors() -> Result<()> {
        let packed = run(
            pack_chain_design(),
            &PackOptions {
                capacity: 4,
                ..PackOptions::default()
            },
        )?
        .value;

        assert_eq!(packed.clusters.len(), 1);
        let members = packed.clusters[0]
            .members
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            members,
            BTreeSet::from([
                "lut0".to_string(),
                "ff0".to_string(),
                "lut1".to_string(),
                "ff1".to_string(),
            ])
        );

        Ok(())
    }

    #[test]
    fn pack_ignores_shared_clock_nets_when_grouping_shift_stages() -> Result<()> {
        let packed = run(
            pack_shared_clock_shift_design(),
            &PackOptions {
                capacity: 4,
                ..PackOptions::default()
            },
        )?
        .value;

        let member_sets = packed
            .clusters
            .iter()
            .map(|cluster| cluster.members.iter().cloned().collect::<BTreeSet<_>>())
            .collect::<BTreeSet<_>>();

        assert_eq!(
            member_sets,
            BTreeSet::from([
                BTreeSet::from([
                    "lut0".to_string(),
                    "ff0".to_string(),
                    "lut1".to_string(),
                    "ff1".to_string(),
                ]),
                BTreeSet::from([
                    "lut2".to_string(),
                    "ff2".to_string(),
                    "lut3".to_string(),
                    "ff3".to_string(),
                ]),
            ])
        );

        Ok(())
    }
}
