use super::{
    PlaceMode, PlaceOptions,
    cost::{PlacementEvaluator, evaluate},
    graph::build_cluster_graph,
    model::PlacementModel,
    run, solver,
};
use crate::{
    ir::{Cell, CellPin, Cluster, Design, Endpoint, Net, Port, PortDirection},
    resource::{Arch, DelayModel},
};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

fn mini_arch() -> Arch {
    Arch {
        name: "mini".to_string(),
        width: 6,
        height: 6,
        slices_per_tile: 2,
        lut_inputs: 4,
        wire_r: 0.04,
        wire_c: 0.03,
        ..Arch::default()
    }
}

fn mini_delay() -> DelayModel {
    DelayModel {
        name: "clb2clb".to_string(),
        width: 6,
        height: 6,
        values: vec![
            vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5],
            vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6],
            vec![0.2, 0.3, 0.4, 0.5, 0.6, 0.7],
            vec![0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
            vec![0.4, 0.5, 0.6, 0.7, 0.8, 0.9],
            vec![0.5, 0.6, 0.7, 0.8, 0.9, 1.0],
        ],
    }
}

fn synthetic_arch(width: usize, height: usize) -> Arch {
    Arch {
        name: format!("synthetic-{width}x{height}"),
        width,
        height,
        slices_per_tile: 2,
        lut_inputs: 4,
        wire_r: 0.04,
        wire_c: 0.03,
        ..Arch::default()
    }
}

fn synthetic_delay(width: usize, height: usize) -> DelayModel {
    let values = (0..height)
        .map(|dy| {
            (0..width)
                .map(|dx| 0.05 * (dx + dy) as f64)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    DelayModel {
        name: format!("synthetic-{width}x{height}"),
        width,
        height,
        values,
    }
}

fn clustered_design() -> Design {
    Design {
        name: "place-mini".to_string(),
        ports: vec![
            Port {
                name: "in".to_string(),
                direction: PortDirection::Input,
                x: Some(0),
                y: Some(2),
                ..Port::default()
            },
            Port {
                name: "out".to_string(),
                direction: PortDirection::Output,
                x: Some(5),
                y: Some(2),
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
                cluster: Some("clb0".to_string()),
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
                cluster: Some("clb1".to_string()),
                ..Cell::default()
            },
            Cell {
                name: "u2".to_string(),
                kind: "lut".to_string(),
                type_name: "LUT4".to_string(),
                inputs: vec![CellPin {
                    port: "A".to_string(),
                    net: "mid1".to_string(),
                }],
                outputs: vec![CellPin {
                    port: "O".to_string(),
                    net: "out_net".to_string(),
                }],
                cluster: Some("clb2".to_string()),
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
                sinks: vec![Endpoint {
                    kind: "cell".to_string(),
                    name: "u0".to_string(),
                    pin: "A".to_string(),
                }],
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
                    kind: "cell".to_string(),
                    name: "u2".to_string(),
                    pin: "A".to_string(),
                }],
                ..Net::default()
            },
            Net {
                name: "out_net".to_string(),
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
        clusters: vec![
            Cluster {
                name: "clb0".to_string(),
                kind: "logic".to_string(),
                members: vec!["u0".to_string()],
                capacity: 1,
                ..Cluster::default()
            },
            Cluster {
                name: "clb1".to_string(),
                kind: "logic".to_string(),
                members: vec!["u1".to_string()],
                capacity: 1,
                ..Cluster::default()
            },
            Cluster {
                name: "clb2".to_string(),
                kind: "logic".to_string(),
                members: vec!["u2".to_string()],
                capacity: 1,
                ..Cluster::default()
            },
        ],
        ..Design::default()
    }
}

fn placed_coordinates(design: &Design) -> Vec<(String, usize, usize)> {
    let mut coords = design
        .clusters
        .iter()
        .map(|cluster| {
            (
                cluster.name.clone(),
                cluster.x.unwrap_or(usize::MAX),
                cluster.y.unwrap_or(usize::MAX),
            )
        })
        .collect::<Vec<_>>();
    coords.sort();
    coords
}

fn connected_pair_design() -> Design {
    Design {
        name: "place-pair".to_string(),
        cells: vec![
            Cell {
                name: "src".to_string(),
                kind: "lut".to_string(),
                type_name: "LUT4".to_string(),
                outputs: vec![CellPin {
                    port: "O".to_string(),
                    net: "link".to_string(),
                }],
                cluster: Some("clb0".to_string()),
                ..Cell::default()
            },
            Cell {
                name: "dst".to_string(),
                kind: "lut".to_string(),
                type_name: "LUT4".to_string(),
                inputs: vec![CellPin {
                    port: "A".to_string(),
                    net: "link".to_string(),
                }],
                cluster: Some("clb1".to_string()),
                ..Cell::default()
            },
        ],
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
            ..Net::default()
        }],
        clusters: vec![
            Cluster {
                name: "clb0".to_string(),
                kind: "logic".to_string(),
                members: vec!["src".to_string()],
                capacity: 1,
                ..Cluster::default()
            },
            Cluster {
                name: "clb1".to_string(),
                kind: "logic".to_string(),
                members: vec!["dst".to_string()],
                capacity: 1,
                ..Cluster::default()
            },
        ],
        ..Design::default()
    }
}

fn large_grid_design(width: usize, height: usize) -> Design {
    let mut cells = Vec::new();
    let mut clusters = Vec::new();
    let mut nets = Vec::new();
    let mut input_nets = vec![vec![Option::<String>::None; width]; height];

    for (y, row_input_nets) in input_nets.iter_mut().enumerate().take(height) {
        for x in 0..width {
            let cell_name = format!("u_{x}_{y}");
            let cluster_name = format!("clb_{x}_{y}");
            let mut inputs = Vec::new();
            if x > 0
                && let Some(net) = &row_input_nets[x]
            {
                inputs.push(CellPin {
                    port: "A".to_string(),
                    net: net.clone(),
                });
            }
            if y > 0 {
                let net = format!("v_{x}_{}", y - 1);
                inputs.push(CellPin {
                    port: "B".to_string(),
                    net,
                });
            }

            let mut outputs = Vec::new();
            if x + 1 < width {
                let net_name = format!("h_{x}_{y}");
                outputs.push(CellPin {
                    port: "OX".to_string(),
                    net: net_name.clone(),
                });
                nets.push(Net {
                    name: net_name.clone(),
                    driver: Some(Endpoint {
                        kind: "cell".to_string(),
                        name: cell_name.clone(),
                        pin: "OX".to_string(),
                    }),
                    sinks: vec![Endpoint {
                        kind: "cell".to_string(),
                        name: format!("u_{}_{}", x + 1, y),
                        pin: "A".to_string(),
                    }],
                    ..Net::default()
                });
                row_input_nets[x + 1] = Some(net_name);
            }
            if y + 1 < height {
                let net_name = format!("v_{x}_{y}");
                outputs.push(CellPin {
                    port: "OY".to_string(),
                    net: net_name.clone(),
                });
                nets.push(Net {
                    name: net_name,
                    driver: Some(Endpoint {
                        kind: "cell".to_string(),
                        name: cell_name.clone(),
                        pin: "OY".to_string(),
                    }),
                    sinks: vec![Endpoint {
                        kind: "cell".to_string(),
                        name: format!("u_{x}_{}", y + 1),
                        pin: "B".to_string(),
                    }],
                    ..Net::default()
                });
            }

            if x + 2 < width && y + 1 < height && (x + y) % 3 == 0 {
                let net_name = format!("d_{x}_{y}");
                outputs.push(CellPin {
                    port: "OD".to_string(),
                    net: net_name.clone(),
                });
                nets.push(Net {
                    name: net_name,
                    driver: Some(Endpoint {
                        kind: "cell".to_string(),
                        name: cell_name.clone(),
                        pin: "OD".to_string(),
                    }),
                    sinks: vec![Endpoint {
                        kind: "cell".to_string(),
                        name: format!("u_{}_{}", x + 2, y + 1),
                        pin: "C".to_string(),
                    }],
                    ..Net::default()
                });
            }

            cells.push(Cell {
                name: cell_name,
                kind: "lut".to_string(),
                type_name: "LUT4".to_string(),
                inputs,
                outputs,
                cluster: Some(cluster_name.clone()),
                ..Cell::default()
            });
            clusters.push(Cluster {
                name: cluster_name,
                kind: "logic".to_string(),
                members: vec![format!("u_{x}_{y}")],
                capacity: 1,
                ..Cluster::default()
            });
        }
    }

    Design {
        name: format!("large-grid-{width}x{height}"),
        cells,
        nets,
        clusters,
        ..Design::default()
    }
}

fn fixed_cluster_design() -> Design {
    let mut design = connected_pair_design();
    if let Some(cluster) = design
        .clusters
        .iter_mut()
        .find(|cluster| cluster.name == "clb0")
    {
        cluster.fixed = true;
        cluster.x = Some(1);
        cluster.y = Some(1);
    }
    design
}

#[test]
fn placement_is_seed_stable_and_legal_in_both_modes() -> Result<()> {
    for mode in [PlaceMode::BoundingBox, PlaceMode::TimingDriven] {
        let options = PlaceOptions {
            arch: mini_arch(),
            delay: Some(mini_delay()),
            constraints: Vec::new(),
            mode,
            seed: 0xCAFE_BABE,
        };
        let first = run(clustered_design(), &options)?.value;
        let second = run(clustered_design(), &options)?.value;

        let first_coords = placed_coordinates(&first);
        let second_coords = placed_coordinates(&second);
        assert_eq!(
            first_coords, second_coords,
            "placement should be deterministic"
        );

        let unique_sites = first_coords
            .iter()
            .map(|(_, x, y)| (*x, *y))
            .collect::<BTreeSet<_>>();
        assert_eq!(unique_sites.len(), first.clusters.len());
        assert!(
            first_coords
                .iter()
                .all(|(_, x, y)| *x > 0 && *x < 5 && *y > 0 && *y < 5)
        );
    }

    Ok(())
}

#[test]
fn strongly_connected_pair_is_placed_adjacent() -> Result<()> {
    for mode in [PlaceMode::BoundingBox, PlaceMode::TimingDriven] {
        let placed = run(
            connected_pair_design(),
            &PlaceOptions {
                arch: mini_arch(),
                delay: Some(mini_delay()),
                constraints: Vec::new(),
                mode,
                seed: 7,
            },
        )?
        .value;

        let coords = placed_coordinates(&placed);
        let lhs = (coords[0].1, coords[0].2);
        let rhs = (coords[1].1, coords[1].2);
        assert_eq!(
            super::manhattan(lhs, rhs),
            1,
            "expected adjacent placement in {mode:?}"
        );
    }

    Ok(())
}

#[test]
fn fixed_clusters_keep_their_requested_site() -> Result<()> {
    let placed = run(
        fixed_cluster_design(),
        &PlaceOptions {
            arch: mini_arch(),
            delay: Some(mini_delay()),
            constraints: Vec::new(),
            mode: PlaceMode::TimingDriven,
            seed: 99,
        },
    )?
    .value;
    let fixed = placed
        .clusters
        .iter()
        .find(|cluster| cluster.name == "clb0")
        .ok_or_else(|| anyhow::anyhow!("missing fixed cluster"))?;
    assert_eq!((fixed.x, fixed.y), (Some(1), Some(1)));
    assert!(fixed.fixed);
    Ok(())
}

#[test]
fn timing_objective_penalizes_stretched_critical_chain_more_strongly() {
    let mut design = clustered_design();
    for net in &mut design.nets {
        net.criticality = match net.name.as_str() {
            "mid0" | "mid1" => 1.0,
            _ => 0.1,
        };
    }
    let graph = build_cluster_graph(&design);
    let model = PlacementModel::from_design(&design);
    let compact = BTreeMap::from([
        ("clb0".to_string(), (1usize, 2usize)),
        ("clb1".to_string(), (2usize, 2usize)),
        ("clb2".to_string(), (3usize, 2usize)),
    ]);
    let stretched = BTreeMap::from([
        ("clb0".to_string(), (1usize, 1usize)),
        ("clb1".to_string(), (3usize, 3usize)),
        ("clb2".to_string(), (4usize, 4usize)),
    ]);

    let bounding_gap = evaluate(
        &model,
        &graph,
        &stretched,
        &mini_arch(),
        Some(&mini_delay()),
        PlaceMode::BoundingBox,
    )
    .total
        - evaluate(
            &model,
            &graph,
            &compact,
            &mini_arch(),
            Some(&mini_delay()),
            PlaceMode::BoundingBox,
        )
        .total;
    let timing_gap = evaluate(
        &model,
        &graph,
        &stretched,
        &mini_arch(),
        Some(&mini_delay()),
        PlaceMode::TimingDriven,
    )
    .total
        - evaluate(
            &model,
            &graph,
            &compact,
            &mini_arch(),
            Some(&mini_delay()),
            PlaceMode::TimingDriven,
        )
        .total;

    assert!(timing_gap > bounding_gap);
}

#[test]
fn incremental_evaluator_matches_full_recompute_for_move_and_swap() {
    let mut design = clustered_design();
    for net in &mut design.nets {
        net.criticality = match net.name.as_str() {
            "mid0" | "mid1" => 1.0,
            _ => 0.2,
        };
    }

    let graph = build_cluster_graph(&design);
    let model = PlacementModel::from_design(&design);
    let placements = BTreeMap::from([
        ("clb0".to_string(), (1usize, 2usize)),
        ("clb1".to_string(), (2usize, 2usize)),
        ("clb2".to_string(), (4usize, 3usize)),
    ]);
    let arch = mini_arch();
    let delay = mini_delay();
    let mode = PlaceMode::TimingDriven;
    let mut evaluator = PlacementEvaluator::new(
        &model,
        &graph,
        placements.clone(),
        &arch,
        Some(&delay),
        mode,
    );

    let move_updates = vec![("clb1".to_string(), (3usize, 1usize))];
    let move_candidate = evaluator.evaluate_candidate(&move_updates);
    let mut moved = placements.clone();
    moved.insert("clb1".to_string(), (3, 1));
    let moved_metrics = evaluate(&model, &graph, &moved, &arch, Some(&delay), mode);
    assert_metrics_close(move_candidate.metrics(), &moved_metrics);

    evaluator.apply_candidate(move_candidate);
    assert_metrics_close(evaluator.metrics(), &moved_metrics);

    let swap_updates = vec![
        ("clb0".to_string(), (4usize, 3usize)),
        ("clb2".to_string(), (1usize, 2usize)),
    ];
    let swap_candidate = evaluator.evaluate_candidate(&swap_updates);
    let mut swapped = moved.clone();
    swapped.insert("clb0".to_string(), (4, 3));
    swapped.insert("clb2".to_string(), (1, 2));
    let swapped_metrics = evaluate(&model, &graph, &swapped, &arch, Some(&delay), mode);
    assert_metrics_close(swap_candidate.metrics(), &swapped_metrics);

    evaluator.apply_candidate(swap_candidate);
    assert_metrics_close(evaluator.metrics(), &swapped_metrics);
}

#[test]
fn large_synthetic_design_places_legally_and_deterministically() -> Result<()> {
    let design = large_grid_design(9, 9);
    let arch = synthetic_arch(14, 14);
    let delay = synthetic_delay(arch.width, arch.height);
    let options = PlaceOptions {
        arch,
        delay: Some(delay),
        constraints: Vec::new(),
        mode: PlaceMode::TimingDriven,
        seed: 0xC0FFEE,
    };

    let placed_a = run(design.clone(), &options)?.value;
    let placed_b = run(design, &options)?.value;
    let coords_a = placed_coordinates(&placed_a);
    let coords_b = placed_coordinates(&placed_b);

    assert_eq!(coords_a, coords_b);

    let unique_sites = coords_a
        .iter()
        .map(|(_, x, y)| (*x, *y))
        .collect::<BTreeSet<_>>();
    assert_eq!(unique_sites.len(), coords_a.len());
    assert!(coords_a.iter().all(|(_, x, y)| *x < 14 && *y < 14));

    Ok(())
}

#[test]
#[ignore = "benchmark-style stress test for larger synthetic placement"]
fn large_synthetic_design_benchmark() -> Result<()> {
    let design = large_grid_design(10, 10);
    let arch = synthetic_arch(15, 15);
    let delay = synthetic_delay(arch.width, arch.height);
    let options = PlaceOptions {
        arch,
        delay: Some(delay),
        constraints: Vec::new(),
        mode: PlaceMode::TimingDriven,
        seed: 0x1234_5678,
    };

    let start_full = Instant::now();
    let full = solver::solve_for_test(&design, &options, false)?;
    let elapsed_full = start_full.elapsed();
    let start_incremental = Instant::now();
    let incremental = solver::solve_for_test(&design, &options, true)?;
    let elapsed_incremental = start_incremental.elapsed();

    assert_eq!(full.placements, incremental.placements);
    assert_metrics_close(&full.metrics, &incremental.metrics);

    eprintln!(
        "large synthetic placement: clusters={} full_ms={} incremental_ms={}",
        full.placements.len(),
        elapsed_full.as_millis(),
        elapsed_incremental.as_millis()
    );
    assert_eq!(full.placements.len(), 10 * 10);
    Ok(())
}

fn assert_metrics_close(lhs: &super::cost::PlacementMetrics, rhs: &super::cost::PlacementMetrics) {
    for (lhs_value, rhs_value) in [
        (lhs.wire_cost, rhs.wire_cost),
        (lhs.congestion_cost, rhs.congestion_cost),
        (lhs.timing_cost, rhs.timing_cost),
        (lhs.locality_cost, rhs.locality_cost),
        (lhs.total, rhs.total),
    ] {
        assert!(
            (lhs_value - rhs_value).abs() < 1e-9,
            "metrics diverged: {lhs_value} vs {rhs_value}"
        );
    }
}
