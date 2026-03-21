use super::{StaOptions, run};
use crate::{
    ir::{Cell, CellPin, Cluster, Design, Endpoint, Net, Port, PortDirection, RouteSegment},
    resource::Arch,
};
use anyhow::Result;

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

fn timed_design() -> Design {
    Design {
        name: "sta-mini".to_string(),
        stage: "routed".to_string(),
        ports: vec![
            Port {
                name: "in".to_string(),
                direction: PortDirection::Input,
                x: Some(0),
                y: Some(1),
                ..Port::default()
            },
            Port {
                name: "out".to_string(),
                direction: PortDirection::Output,
                x: Some(3),
                y: Some(1),
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
                route: vec![RouteSegment {
                    x0: 0,
                    y0: 1,
                    x1: 1,
                    y1: 1,
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
                route: vec![
                    RouteSegment {
                        x0: 1,
                        y0: 1,
                        x1: 2,
                        y1: 1,
                    },
                    RouteSegment {
                        x0: 2,
                        y0: 1,
                        x1: 2,
                        y1: 2,
                    },
                ],
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
                route: vec![RouteSegment {
                    x0: 2,
                    y0: 2,
                    x1: 3,
                    y1: 2,
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
                y: Some(1),
                ..Cluster::default()
            },
            Cluster {
                name: "clb1".to_string(),
                kind: "logic".to_string(),
                members: vec!["u1".to_string()],
                capacity: 1,
                x: Some(2),
                y: Some(2),
                ..Cluster::default()
            },
        ],
        ..Design::default()
    }
}

#[test]
fn sta_computes_expected_critical_path_and_graph_shape() -> Result<()> {
    let artifact = run(
        timed_design(),
        &StaOptions {
            arch: Some(mini_arch()),
            delay: None,
        },
    )?
    .value;

    let summary = artifact.design.timing.expect("timing summary");
    assert!((summary.critical_path_ns - 0.79).abs() < 1e-9);
    assert!((summary.fmax_mhz - (1_000.0 / 0.79)).abs() < 1e-9);
    assert_eq!(
        summary.top_paths.first().map(|path| path.endpoint.as_str()),
        Some("out:OUT")
    );
    assert!(artifact.report_text.contains("Critical Path: 0.790 ns"));
    assert_eq!(artifact.graph.edges.len(), 5);
    assert!(
        artifact
            .graph
            .nodes
            .iter()
            .any(|node| node.id == "port:out:OUT")
    );

    Ok(())
}
