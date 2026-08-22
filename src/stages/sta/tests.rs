use super::{StaOptions, run, run_with_reporter};
use crate::{
    domain::TimingPathCategory,
    ir::{Cell, Cluster, Design, Endpoint, Net, Port, RouteSegment},
    report::{StageEvent, StageReporter, run_stage_with_reporter},
    resource::{Arch, DelayModel},
};
use anyhow::Result;
use std::sync::{Arc, Mutex};

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

#[test]
fn sta_sequential_output_restarts_arrival() -> Result<()> {
    let design = Design {
        name: "sta-ff".to_string(),
        stage: "routed".to_string(),
        ports: vec![Port::input("in").at(0, 1), Port::output("out").at(3, 1)],
        cells: vec![
            Cell::lut("u_lut", "LUT4")
                .with_input("A", "in_net")
                .with_output("O", "mid_net")
                .in_cluster("clb0"),
            Cell::ff("u_ff", "FDCE")
                .with_input("D", "mid_net")
                .with_output("Q", "q_net")
                .in_cluster("clb1"),
        ],
        nets: vec![
            Net::new("in_net")
                .with_driver(Endpoint::port("in", "IN"))
                .with_sink(Endpoint::cell("u_lut", "A"))
                .with_route_segment(RouteSegment::new((0, 1), (1, 1))),
            Net::new("mid_net")
                .with_driver(Endpoint::cell("u_lut", "O"))
                .with_sink(Endpoint::cell("u_ff", "D"))
                .with_route_segment(RouteSegment::new((1, 1), (2, 2))),
            Net::new("q_net")
                .with_driver(Endpoint::cell("u_ff", "Q"))
                .with_sink(Endpoint::port("out", "OUT"))
                .with_route_segment(RouteSegment::new((2, 2), (3, 2))),
        ],
        clusters: vec![
            Cluster::logic("clb0")
                .with_member("u_lut")
                .with_capacity(1)
                .at(1, 1),
            Cluster::logic("clb1")
                .with_member("u_ff")
                .with_capacity(1)
                .at(2, 2),
        ],
        ..Design::default()
    };

    let artifact = run(
        design,
        &StaOptions {
            arch: Some(mini_arch().into()),
            delay: None,
        },
    )?
    .value;

    // in_net 0.09 + LUT 0.19 = 0.28 at u_lut:O; mid_net adds 0.18 -> 0.46 at
    // u_ff:D. The FF restarts arrivals at 0.2, so the output port only sees
    // 0.2 + q_net 0.09 = 0.29 instead of accumulating through the register.
    let summary = artifact.design.timing.expect("timing summary");
    assert!((summary.critical_path_ns - 0.46).abs() < 1e-9);
    assert!((summary.fmax_mhz - 1_000.0 / 0.46).abs() < 1e-9);
    assert_eq!(
        summary.top_paths.first().map(|path| path.category),
        Some(TimingPathCategory::RegisterInput)
    );
    assert_eq!(
        summary.top_paths.get(1).map(|path| path.category),
        Some(TimingPathCategory::PrimaryOutput)
    );

    Ok(())
}

#[test]
fn sta_takes_slowest_fanin_arrival() -> Result<()> {
    let design = Design {
        name: "sta-fanin".to_string(),
        stage: "routed".to_string(),
        ports: vec![
            Port::input("in_a").at(0, 0),
            Port::input("in_b").at(0, 5),
            Port::output("out").at(4, 2),
        ],
        cells: vec![
            Cell::lut("u0", "LUT4")
                .with_input("A", "a_net")
                .with_input("B", "b_net")
                .with_output("O", "out_net")
                .in_cluster("clb0"),
        ],
        nets: vec![
            Net::new("a_net")
                .with_driver(Endpoint::port("in_a", "IN"))
                .with_sink(Endpoint::cell("u0", "A")),
            Net::new("b_net")
                .with_driver(Endpoint::port("in_b", "IN"))
                .with_sink(Endpoint::cell("u0", "B")),
            Net::new("out_net")
                .with_driver(Endpoint::cell("u0", "O"))
                .with_sink(Endpoint::port("out", "OUT")),
        ],
        clusters: vec![
            Cluster::logic("clb0")
                .with_member("u0")
                .with_capacity(1)
                .at(2, 2),
        ],
        ..Design::default()
    };

    let artifact = run(design, &StaOptions::default())?.value;

    // No routes: Manhattan fallback. a_net (2,2) -> 0.32, b_net (2,3) ->
    // 0.40; the LUT takes the slower fan-in 0.40 + 0.23 intrinsic = 0.63,
    // and out_net (2,0) adds 0.16 for 0.79 at the primary output.
    let summary = artifact.design.timing.expect("timing summary");
    assert!((summary.critical_path_ns - 0.79).abs() < 1e-9);
    let delays = summary
        .top_paths
        .iter()
        .map(|path| path.delay_ns)
        .collect::<Vec<_>>();
    assert_eq!(delays.len(), 3);
    assert!((delays[0] - 0.79).abs() < 1e-9);
    assert!((delays[1] - 0.40).abs() < 1e-9);
    assert!((delays[2] - 0.32).abs() < 1e-9);

    Ok(())
}

#[test]
fn sta_delay_model_overrides_distance_fallback() -> Result<()> {
    let design = Design {
        name: "sta-model".to_string(),
        stage: "routed".to_string(),
        ports: vec![Port::input("in").at(0, 0), Port::output("out").at(3, 1)],
        cells: vec![
            Cell::lut("u0", "LUT4")
                .with_input("A", "in_net")
                .with_output("O", "out_net")
                .in_cluster("clb0"),
        ],
        nets: vec![
            Net::new("in_net")
                .with_driver(Endpoint::port("in", "IN"))
                .with_sink(Endpoint::cell("u0", "A")),
            Net::new("out_net")
                .with_driver(Endpoint::cell("u0", "O"))
                .with_sink(Endpoint::port("out", "OUT")),
        ],
        clusters: vec![
            Cluster::logic("clb0")
                .with_member("u0")
                .with_capacity(1)
                .at(1, 1),
        ],
        ..Design::default()
    };
    let model = DelayModel {
        name: "table".to_string(),
        width: 2,
        height: 2,
        values: vec![vec![0.5, 0.6], vec![0.7, 0.8]],
    };

    let without_model = run(design.clone(), &StaOptions::default())?.value;
    assert!(
        (without_model
            .design
            .timing
            .expect("manhattan timing")
            .critical_path_ns
            - 0.51)
            .abs()
            < 1e-9,
        "distance fallback should give 0.16 + 0.19 + 0.16"
    );

    let with_model = run(
        design,
        &StaOptions {
            arch: None,
            delay: Some(model.into()),
        },
    )?
    .value;
    // lookup(1,1) = values[1][1] = 0.8 and lookup(2,0) clamps to
    // values[0][1] = 0.6: 0.8 + 0.19 + 0.6 = 1.59.
    let summary = with_model.design.timing.expect("model timing");
    assert!((summary.critical_path_ns - 1.59).abs() < 1e-9);

    Ok(())
}

#[test]
fn sta_nan_delay_surfaces_typed_error() {
    let design = Design {
        name: "sta-nan".to_string(),
        stage: "routed".to_string(),
        ports: vec![Port::input("in").at(0, 0), Port::output("out").at(3, 1)],
        cells: vec![
            Cell::lut("u0", "LUT4")
                .with_input("A", "in_net")
                .with_output("O", "out_net")
                .in_cluster("clb0"),
        ],
        nets: vec![
            Net::new("in_net")
                .with_driver(Endpoint::port("in", "IN"))
                .with_sink(Endpoint::cell("u0", "A")),
            Net::new("out_net")
                .with_driver(Endpoint::cell("u0", "O"))
                .with_sink(Endpoint::port("out", "OUT")),
        ],
        clusters: vec![
            Cluster::logic("clb0")
                .with_member("u0")
                .with_capacity(1)
                .at(1, 1),
        ],
        ..Design::default()
    };
    let model = DelayModel {
        name: "broken".to_string(),
        width: 1,
        height: 1,
        values: vec![vec![f64::NAN]],
    };

    let error = run(
        design,
        &StaOptions {
            arch: None,
            delay: Some(model.into()),
        },
    )
    .expect_err("nan delay must fail");

    assert!(matches!(error, super::StaError::NonFiniteArrival { .. }));
}

#[test]
fn sta_empty_design_reports_zero_critical_path() -> Result<()> {
    let artifact = run(
        Design {
            name: "sta-empty".to_string(),
            stage: "routed".to_string(),
            ..Design::default()
        },
        &StaOptions::default(),
    )?
    .value;

    let summary = artifact.design.timing.expect("timing summary");
    assert_eq!(summary.critical_path_ns, 0.0);
    assert_eq!(summary.fmax_mhz, 0.0);
    assert!(summary.top_paths.is_empty());
    assert!(artifact.graph.nodes.is_empty());
    assert!(artifact.graph.edges.is_empty());
    assert!(artifact.report_text.contains("Critical Path: 0.000 ns"));

    Ok(())
}

#[test]
fn sta_reporter_receives_stage_events() -> Result<()> {
    let events = Arc::new(Mutex::new(Vec::<StageEvent>::new()));
    struct Collector(Arc<Mutex<Vec<StageEvent>>>);

    impl StageReporter for Collector {
        fn on_stage_event(&mut self, event: StageEvent) {
            self.0.lock().expect("event lock").push(event);
        }
    }

    let mut reporter = Some(&mut Collector(Arc::clone(&events)) as &mut dyn StageReporter);
    run_stage_with_reporter(
        "sta",
        &mut reporter,
        || run(timed_design(), &StaOptions::default()),
        |reporter| run_with_reporter(timed_design(), &StaOptions::default(), reporter),
    )?;

    let events = events.lock().expect("event lock");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, StageEvent::Started { stage } if *stage == "sta"))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, StageEvent::Finished { stage, .. } if *stage == "sta"))
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            StageEvent::Log { message, .. }
            if message.contains("STA model")
        )),
        "arrival info event missing: {events:?}"
    );

    Ok(())
}
