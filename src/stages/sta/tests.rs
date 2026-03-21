use super::{StaOptions, run};
use crate::{
    ir::{Cell, Cluster, Design, Endpoint, Net, Port, RouteSegment},
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
        ports: vec![Port::input("in").at(0, 1), Port::output("out").at(3, 1)],
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
                .with_sink(Endpoint::cell("u0", "A"))
                .with_route_segment(RouteSegment::new((0, 1), (1, 1))),
            Net::new("mid_net")
                .with_driver(Endpoint::cell("u0", "O"))
                .with_sink(Endpoint::cell("u1", "A"))
                .with_route_segment(RouteSegment::new((1, 1), (2, 1)))
                .with_route_segment(RouteSegment::new((2, 1), (2, 2))),
            Net::new("out_net")
                .with_driver(Endpoint::cell("u1", "O"))
                .with_sink(Endpoint::port("out", "OUT"))
                .with_route_segment(RouteSegment::new((2, 2), (3, 2))),
        ],
        clusters: vec![
            Cluster::logic("clb0")
                .with_member("u0")
                .with_capacity(1)
                .at(1, 1),
            Cluster::logic("clb1")
                .with_member("u1")
                .with_capacity(1)
                .at(2, 2),
        ],
        ..Design::default()
    }
}

#[test]
fn sta_computes_expected_critical_path_and_graph_shape() -> Result<()> {
    let artifact = run(
        timed_design(),
        &StaOptions {
            arch: Some(mini_arch().into()),
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
