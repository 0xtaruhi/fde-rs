use super::{
    EdgeKey, RouteMode, RouteOptions, canonical_edge, points_to_edges, route_congestion_penalty,
    route_single_net, run,
    search::{astar_to_tree, tree_distance_field},
};
use crate::{
    ir::{Cell, CellPin, Cluster, Design, Endpoint, Net, Port, PortDirection},
    resource::{Arch, TileInstance, TileSideCapacity},
    route::cost::search_profile,
};
use anyhow::Result;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeMap;

fn mini_arch() -> Arch {
    Arch {
        name: "mini".to_string(),
        width: 6,
        height: 6,
        slices_per_tile: 2,
        lut_inputs: 4,
        wire_r: 0.04,
        wire_c: 0.03,
        default_horizontal_capacity: 1,
        default_vertical_capacity: 1,
        ..Arch::default()
    }
}

fn multi_track_arch(horizontal_capacity: usize, vertical_capacity: usize) -> Arch {
    let mut arch = mini_arch();
    arch.default_horizontal_capacity = horizontal_capacity;
    arch.default_vertical_capacity = vertical_capacity;
    arch.tile_side_capacities.insert(
        "logic".to_string(),
        TileSideCapacity {
            left: horizontal_capacity,
            right: horizontal_capacity,
            bottom: vertical_capacity,
            top: vertical_capacity,
        },
    );
    for x in 0..arch.width {
        for y in 0..arch.height {
            arch.tiles.insert(
                (x, y),
                TileInstance {
                    name: format!("TILE_X{x}_Y{y}"),
                    tile_type: "logic".to_string(),
                    logic_x: x,
                    logic_y: y,
                    bit_x: x,
                    bit_y: y,
                    phy_x: x,
                    phy_y: y,
                },
            );
        }
    }
    arch
}

fn routed_design() -> Design {
    Design {
        name: "route-mini".to_string(),
        stage: "placed".to_string(),
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
                    net: "mid_net".to_string(),
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
                    net: "mid_net".to_string(),
                }],
                outputs: vec![CellPin {
                    port: "O".to_string(),
                    net: "out_net".to_string(),
                }],
                cluster: Some("clb1".to_string()),
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
                name: "mid_net".to_string(),
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
                name: "out_net".to_string(),
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
        ],
        clusters: vec![
            Cluster {
                name: "clb0".to_string(),
                kind: "logic".to_string(),
                members: vec!["u0".to_string()],
                capacity: 1,
                x: Some(1),
                y: Some(2),
                ..Cluster::default()
            },
            Cluster {
                name: "clb1".to_string(),
                kind: "logic".to_string(),
                members: vec!["u1".to_string()],
                capacity: 1,
                x: Some(3),
                y: Some(2),
                ..Cluster::default()
            },
        ],
        ..Design::default()
    }
}

fn congested_port_design() -> Design {
    Design {
        name: "route-congested".to_string(),
        stage: "placed".to_string(),
        ports: vec![
            Port {
                name: "left_a".to_string(),
                direction: PortDirection::Input,
                x: Some(0),
                y: Some(2),
                ..Port::default()
            },
            Port {
                name: "right_a".to_string(),
                direction: PortDirection::Output,
                x: Some(5),
                y: Some(2),
                ..Port::default()
            },
            Port {
                name: "left_b".to_string(),
                direction: PortDirection::Output,
                x: Some(0),
                y: Some(2),
                ..Port::default()
            },
            Port {
                name: "right_b".to_string(),
                direction: PortDirection::Input,
                x: Some(5),
                y: Some(2),
                ..Port::default()
            },
        ],
        nets: vec![
            Net {
                name: "net_a".to_string(),
                driver: Some(Endpoint {
                    kind: "port".to_string(),
                    name: "left_a".to_string(),
                    pin: "OUT".to_string(),
                }),
                sinks: vec![Endpoint {
                    kind: "port".to_string(),
                    name: "right_a".to_string(),
                    pin: "IN".to_string(),
                }],
                ..Net::default()
            },
            Net {
                name: "net_b".to_string(),
                driver: Some(Endpoint {
                    kind: "port".to_string(),
                    name: "right_b".to_string(),
                    pin: "OUT".to_string(),
                }),
                sinks: vec![Endpoint {
                    kind: "port".to_string(),
                    name: "left_b".to_string(),
                    pin: "IN".to_string(),
                }],
                ..Net::default()
            },
        ],
        ..Design::default()
    }
}

fn design_overflow(design: &Design, arch: &Arch) -> usize {
    let mut usage = BTreeMap::<EdgeKey, usize>::new();
    for net in &design.nets {
        for segment in &net.route {
            let edge = canonical_edge((segment.x0, segment.y0), (segment.x1, segment.y1));
            *usage.entry(edge).or_insert(0) += 1;
        }
    }
    usage
        .into_iter()
        .map(|(edge, count)| count.saturating_sub(arch.edge_capacity(edge.0, edge.1)))
        .sum()
}

fn single_net_design(start: (usize, usize), goal: (usize, usize)) -> Design {
    Design {
        name: "route-single".to_string(),
        stage: "placed".to_string(),
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
                x: Some(start.0),
                y: Some(start.1),
                ..Cluster::default()
            },
            Cluster {
                name: "clb1".to_string(),
                kind: "logic".to_string(),
                members: vec!["dst".to_string()],
                capacity: 1,
                x: Some(goal.0),
                y: Some(goal.1),
                ..Cluster::default()
            },
        ],
        ..Design::default()
    }
}

#[test]
fn all_route_modes_produce_segments_and_delay() -> Result<()> {
    for mode in [
        RouteMode::BreadthFirst,
        RouteMode::Directed,
        RouteMode::TimingDriven,
    ] {
        let result = run(
            routed_design(),
            &RouteOptions {
                arch: mini_arch(),
                constraints: Vec::new(),
                mode,
            },
        )?
        .value;

        for net in &result.nets {
            assert_eq!(
                net.route_length(),
                match net.name.as_str() {
                    "in_net" => 1,
                    "mid_net" => 2,
                    "out_net" => 2,
                    _ => 0,
                },
                "expected Manhattan-short route for {} in {:?}",
                net.name,
                mode
            );
            assert!(
                net.estimated_delay_ns > 0.0,
                "expected positive delay for {} in {:?}",
                net.name,
                mode
            );
        }
    }

    Ok(())
}

#[test]
fn route_modes_match_manhattan_shortest_path_on_random_single_net_cases() -> Result<()> {
    let mut rng = ChaCha8Rng::seed_from_u64(0xBEEF);
    for mode in [
        RouteMode::BreadthFirst,
        RouteMode::Directed,
        RouteMode::TimingDriven,
    ] {
        for _ in 0..24 {
            let start = (rng.random_range(0..6), rng.random_range(0..6));
            let mut goal = (rng.random_range(0..6), rng.random_range(0..6));
            while goal == start {
                goal = (rng.random_range(0..6), rng.random_range(0..6));
            }

            let routed = run(
                single_net_design(start, goal),
                &RouteOptions {
                    arch: mini_arch(),
                    constraints: Vec::new(),
                    mode,
                },
            )?
            .value;
            let net = routed.nets.first().expect("single routed net");
            assert_eq!(
                net.route_length(),
                super::manhattan(start, goal),
                "expected shortest path for {start:?} -> {goal:?} in {mode:?}"
            );
        }
    }

    Ok(())
}

#[test]
fn negotiated_search_detours_away_from_hot_edges() {
    let arch = mini_arch();
    let tree = [(4usize, 2usize)];
    let mut tree_mask = vec![false; arch.width * arch.height];
    tree_mask[2 * arch.width + 4] = true;
    let tree_distance = tree_distance_field(&tree, &arch);
    let mut usage = BTreeMap::<EdgeKey, usize>::new();
    for edge in [
        canonical_edge((0, 2), (1, 2)),
        canonical_edge((1, 2), (2, 2)),
        canonical_edge((2, 2), (3, 2)),
        canonical_edge((3, 2), (4, 2)),
    ] {
        usage.insert(edge, 3);
    }

    let path = astar_to_tree(
        (0, 2),
        &tree_mask,
        &tree_distance,
        &arch,
        &usage,
        &BTreeMap::new(),
        search_profile(RouteMode::Directed, 0.0, 0),
    )
    .expect("astar path");
    let path_edges = points_to_edges(&path);
    assert!(!path_edges.iter().all(|edge| usage.contains_key(edge)));
    assert!(path.len() - 1 > 4);
}

#[test]
fn tree_router_reuses_existing_connection_for_multi_sink_net() {
    let net = super::NetPoints {
        driver: (2, 2),
        sinks: vec![(5, 3), (4, 4)],
    };
    let paths = route_single_net(
        &net,
        &mini_arch(),
        RouteMode::TimingDriven,
        0.8,
        &BTreeMap::new(),
        &BTreeMap::new(),
        0,
    )
    .expect("tree route");
    let total_edges = paths
        .iter()
        .map(|path| path.len().saturating_sub(1))
        .sum::<usize>();
    let independent =
        super::manhattan(net.driver, net.sinks[0]) + super::manhattan(net.driver, net.sinks[1]);
    assert!(total_edges < independent);
}

#[test]
fn negotiated_routing_reduces_overflow_against_breadth_first() -> Result<()> {
    let arch = mini_arch();
    let breadth = run(
        congested_port_design(),
        &RouteOptions {
            arch: arch.clone(),
            constraints: Vec::new(),
            mode: RouteMode::BreadthFirst,
        },
    )?
    .value;
    let directed = run(
        congested_port_design(),
        &RouteOptions {
            arch: arch.clone(),
            constraints: Vec::new(),
            mode: RouteMode::Directed,
        },
    )?
    .value;

    assert!(design_overflow(&directed, &arch) < design_overflow(&breadth, &arch));
    Ok(())
}

#[test]
fn channel_capacity_model_allows_multiple_nets_to_share_an_edge() -> Result<()> {
    let arch = multi_track_arch(2, 1);
    let routed = run(
        congested_port_design(),
        &RouteOptions {
            arch: arch.clone(),
            constraints: Vec::new(),
            mode: RouteMode::BreadthFirst,
        },
    )?
    .value;

    assert_eq!(design_overflow(&routed, &arch), 0);
    Ok(())
}

#[test]
fn congested_edges_add_route_delay_penalty() {
    let route = vec![
        crate::ir::RouteSegment {
            x0: 0,
            y0: 0,
            x1: 1,
            y1: 0,
        },
        crate::ir::RouteSegment {
            x0: 1,
            y0: 0,
            x1: 2,
            y1: 0,
        },
    ];
    let usage = BTreeMap::from([
        (canonical_edge((0, 0), (1, 0)), 1usize),
        (canonical_edge((1, 0), (2, 0)), 3usize),
    ]);

    let penalty = route_congestion_penalty(&route, &usage, &mini_arch());
    assert!(penalty > 0.0);
}
