use super::{
    EdgeKey, RouteMode, RouteOptions, canonical_edge, points_to_edges, route_congestion_penalty,
    route_single_net, run,
    search::{astar_to_tree, tree_distance_field},
};
use crate::{
    ir::{Cell, Cluster, Design, Endpoint, Net, Port},
    resource::{Arch, TileInstance, TileKind, TileSideCapacity},
    route::cost::search_profile,
};
use anyhow::Result;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeMap;

fn point(x: usize, y: usize) -> super::GridPoint {
    (x, y).into()
}

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
    let logic_tile_type = TileKind::Logic
        .canonical_name()
        .expect("logic tile type has canonical name")
        .to_string();
    let mut arch = mini_arch();
    arch.default_horizontal_capacity = horizontal_capacity;
    arch.default_vertical_capacity = vertical_capacity;
    arch.tile_side_capacities.insert(
        logic_tile_type.clone(),
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
                    tile_type: logic_tile_type.clone(),
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
        ports: vec![Port::input("in").at(0, 2), Port::output("out").at(5, 2)],
        cells: vec![
            Cell::lut("u0", "LUT4")
                .with_input("A", "in_net")
                .with_output("O", "mid_net")
                .in_cluster("clb0"),
            Cell::lut("u1", "LUT4")
                .with_input("A", "mid_net")
                .with_output("O", "out_net")
                .in_cluster("clb1"),
        ],
        nets: vec![
            Net::new("in_net")
                .with_driver(Endpoint::port("in", "IN"))
                .with_sink(Endpoint::cell("u0", "A")),
            Net::new("mid_net")
                .with_driver(Endpoint::cell("u0", "O"))
                .with_sink(Endpoint::cell("u1", "A")),
            Net::new("out_net")
                .with_driver(Endpoint::cell("u1", "O"))
                .with_sink(Endpoint::port("out", "OUT")),
        ],
        clusters: vec![
            Cluster::logic("clb0")
                .with_member("u0")
                .with_capacity(1)
                .at(1, 2),
            Cluster::logic("clb1")
                .with_member("u1")
                .with_capacity(1)
                .at(3, 2),
        ],
        ..Design::default()
    }
}

fn congested_port_design() -> Design {
    Design {
        name: "route-congested".to_string(),
        stage: "placed".to_string(),
        ports: vec![
            Port::input("left_a").at(0, 2),
            Port::output("right_a").at(5, 2),
            Port::output("left_b").at(0, 2),
            Port::input("right_b").at(5, 2),
        ],
        nets: vec![
            Net::new("net_a")
                .with_driver(Endpoint::port("left_a", "OUT"))
                .with_sink(Endpoint::port("right_a", "IN")),
            Net::new("net_b")
                .with_driver(Endpoint::port("right_b", "OUT"))
                .with_sink(Endpoint::port("left_b", "IN")),
        ],
        ..Design::default()
    }
}

fn timing_pressure_design() -> Design {
    Design {
        name: "route-timing-pressure".to_string(),
        stage: "placed".to_string(),
        ports: vec![
            Port::input("logic_in").at(0, 2),
            Port::output("logic_out").at(5, 2),
            Port::input("left_a").at(0, 2),
            Port::output("right_a").at(5, 2),
            Port::output("left_b").at(0, 2),
            Port::input("right_b").at(5, 2),
        ],
        cells: vec![
            Cell::lut("src", "LUT4")
                .with_input("A", "c_feed")
                .with_output("O", "z_crit")
                .in_cluster("clb0"),
            Cell::lut("dst", "LUT4")
                .with_input("A", "z_crit")
                .with_output("O", "y_out")
                .in_cluster("clb1"),
        ],
        nets: vec![
            Net::new("a_low")
                .with_driver(Endpoint::port("left_a", "OUT"))
                .with_sink(Endpoint::port("right_a", "IN")),
            Net::new("b_low")
                .with_driver(Endpoint::port("right_b", "OUT"))
                .with_sink(Endpoint::port("left_b", "IN")),
            Net::new("c_feed")
                .with_driver(Endpoint::port("logic_in", "OUT"))
                .with_sink(Endpoint::cell("src", "A")),
            Net::new("y_out")
                .with_driver(Endpoint::cell("dst", "O"))
                .with_sink(Endpoint::port("logic_out", "IN")),
            Net::new("z_crit")
                .with_driver(Endpoint::cell("src", "O"))
                .with_sink(Endpoint::cell("dst", "A")),
        ],
        clusters: vec![
            Cluster::logic("clb0")
                .with_member("src")
                .with_capacity(1)
                .at(1, 2),
            Cluster::logic("clb1")
                .with_member("dst")
                .with_capacity(1)
                .at(4, 2),
        ],
        ..Design::default()
    }
}

fn design_overflow(design: &Design, arch: &Arch) -> usize {
    let mut usage = BTreeMap::<EdgeKey, usize>::new();
    for net in &design.nets {
        for segment in &net.route {
            let edge = canonical_edge(point(segment.x0, segment.y0), point(segment.x1, segment.y1));
            *usage.entry(edge).or_insert(0) += 1;
        }
    }
    usage
        .into_iter()
        .map(|(edge, count)| count.saturating_sub(arch.edge_capacity(edge.0.into(), edge.1.into())))
        .sum()
}

fn single_net_design(start: (usize, usize), goal: (usize, usize)) -> Design {
    Design {
        name: "route-single".to_string(),
        stage: "placed".to_string(),
        cells: vec![
            Cell::lut("src", "LUT4")
                .with_output("O", "link")
                .in_cluster("clb0"),
            Cell::lut("dst", "LUT4")
                .with_input("A", "link")
                .in_cluster("clb1"),
        ],
        nets: vec![
            Net::new("link")
                .with_driver(Endpoint::cell("src", "O"))
                .with_sink(Endpoint::cell("dst", "A")),
        ],
        clusters: vec![
            Cluster::logic("clb0")
                .with_member("src")
                .with_capacity(1)
                .at(start.0, start.1),
            Cluster::logic("clb1")
                .with_member("dst")
                .with_capacity(1)
                .at(goal.0, goal.1),
        ],
        ..Design::default()
    }
}

fn expected_route_length(net_name: &str) -> Option<usize> {
    [("in_net", 1usize), ("mid_net", 2), ("out_net", 2)]
        .into_iter()
        .find(|(name, _)| *name == net_name)
        .map(|(_, length)| length)
}

fn net_by_name<'a>(design: &'a Design, name: &str) -> &'a Net {
    design
        .nets
        .iter()
        .find(|net| net.name == name)
        .unwrap_or_else(|| panic!("missing net {name}"))
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
                arch: mini_arch().into(),
                constraints: Vec::new().into(),
                mode,
            },
        )?
        .value;

        for net in &result.nets {
            assert_eq!(
                net.route_length(),
                expected_route_length(&net.name).unwrap_or_default(),
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
                    arch: mini_arch().into(),
                    constraints: Vec::new().into(),
                    mode,
                },
            )?
            .value;
            let net = routed.nets.first().expect("single routed net");
            assert_eq!(
                net.route_length(),
                super::manhattan(start.into(), goal.into()),
                "expected shortest path for {start:?} -> {goal:?} in {mode:?}"
            );
        }
    }

    Ok(())
}

#[test]
fn negotiated_search_detours_away_from_hot_edges() {
    let arch = mini_arch();
    let tree = [point(4, 2)];
    let mut tree_mask = vec![false; arch.width * arch.height];
    tree_mask[2 * arch.width + 4] = true;
    let tree_distance = tree_distance_field(&tree, &arch);
    let mut usage = BTreeMap::<EdgeKey, usize>::new();
    for edge in [
        canonical_edge(point(0, 2), point(1, 2)),
        canonical_edge(point(1, 2), point(2, 2)),
        canonical_edge(point(2, 2), point(3, 2)),
        canonical_edge(point(3, 2), point(4, 2)),
    ] {
        usage.insert(edge, 3);
    }

    let path = astar_to_tree(
        point(0, 2),
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
        driver: point(2, 2),
        sinks: vec![point(5, 3), point(4, 4)],
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
            arch: arch.clone().into(),
            constraints: Vec::new().into(),
            mode: RouteMode::BreadthFirst,
        },
    )?
    .value;
    let directed = run(
        congested_port_design(),
        &RouteOptions {
            arch: arch.clone().into(),
            constraints: Vec::new().into(),
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
            arch: arch.clone().into(),
            constraints: Vec::new().into(),
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
        crate::ir::RouteSegment::new((0, 0), (1, 0)),
        crate::ir::RouteSegment::new((1, 0), (2, 0)),
    ];
    let usage = BTreeMap::from([
        (canonical_edge(point(0, 0), point(1, 0)), 1usize),
        (canonical_edge(point(1, 0), point(2, 0)), 3usize),
    ]);

    let penalty = route_congestion_penalty(&route, &usage, &mini_arch());
    assert!(penalty > 0.0);
}

#[test]
fn timing_reroute_selection_includes_zero_overflow_critical_net() {
    let arch = mini_arch();
    let mut design = Design {
        nets: vec![Net::new("a_low"), Net::new("b_low"), Net::new("z_crit")],
        ..Design::default()
    };
    design.nets[0].criticality = 0.15;
    design.nets[1].criticality = 0.20;
    design.nets[2].criticality = 1.0;

    let point_routes = vec![
        vec![vec![point(0, 1), point(1, 1), point(2, 1), point(3, 1)]],
        vec![vec![point(0, 4), point(1, 4), point(2, 4), point(3, 4)]],
        vec![vec![
            point(1, 2),
            point(1, 3),
            point(1, 4),
            point(1, 5),
            point(2, 5),
            point(3, 5),
            point(3, 4),
            point(3, 3),
            point(3, 2),
        ]],
    ];
    let usage = super::point_route_usage_dense(&point_routes, &arch);
    let net_overflow = super::net_overflow_counts(&point_routes, &usage, &arch);

    let reroute = super::select_reroute_nets(
        &design,
        &point_routes,
        &usage,
        &net_overflow,
        &arch,
        RouteMode::TimingDriven,
    );
    assert_eq!(reroute, vec![2]);
}

#[test]
fn timing_driven_shortens_critical_net_under_congestion() -> Result<()> {
    let arch = mini_arch();
    let directed = run(
        timing_pressure_design(),
        &RouteOptions {
            arch: arch.clone().into(),
            constraints: Vec::new().into(),
            mode: RouteMode::Directed,
        },
    )?
    .value;
    let timing = run(
        timing_pressure_design(),
        &RouteOptions {
            arch: arch.clone().into(),
            constraints: Vec::new().into(),
            mode: RouteMode::TimingDriven,
        },
    )?
    .value;

    let directed_critical = net_by_name(&directed, "z_crit");
    let timing_critical = net_by_name(&timing, "z_crit");

    assert!(design_overflow(&timing, &arch) <= design_overflow(&directed, &arch));
    assert!(timing_critical.route_length() < directed_critical.route_length());
    assert!(timing_critical.estimated_delay_ns < directed_critical.estimated_delay_ns);
    assert_eq!(timing_critical.route_length(), 3);
    Ok(())
}
