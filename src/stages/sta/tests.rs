use super::{StaOptions, StaTimingContext, run, run_with_reporter, run_with_timing};
use crate::{
    constraints::{ClockConstraint, ClockUncertaintyConstraint, IoDelayConstraint},
    domain::TimingPathCategory,
    ir::{Cell, Cluster, Design, Endpoint, Net, Port, RouteSegment},
    report::{StageEvent, StageReporter, run_stage_with_reporter},
    resource::{Arch, CellTimingModel, DelayModel, SequentialTiming},
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

fn register_path_design(
    launch: &str,
    logic: &str,
    capture: &str,
    q_net: &str,
    data_net: &str,
) -> Design {
    Design {
        name: "sta-constrained".to_string(),
        stage: "routed".to_string(),
        ports: vec![Port::input("clk").at(0, 0)],
        cells: vec![
            Cell::ff(launch, "DFFHQ")
                .with_input("CK", "clk_net")
                .with_output("Q", q_net)
                .in_cluster("clb0"),
            Cell::lut(logic, "LUT4")
                .with_input("A", q_net)
                .with_output("O", data_net)
                .in_cluster("clb1"),
            Cell::ff(capture, "DFFHQ")
                .with_input("D", data_net)
                .with_input("CK", "clk_net")
                .in_cluster("clb2"),
        ],
        nets: vec![
            Net::new("clk_net")
                .with_driver(Endpoint::port("clk", "IN"))
                .with_sink(Endpoint::cell(launch, "CK"))
                .with_sink(Endpoint::cell(capture, "CK")),
            Net::new(q_net)
                .with_driver(Endpoint::cell(launch, "Q"))
                .with_sink(Endpoint::cell(logic, "A"))
                .with_route_segment(RouteSegment::new((1, 1), (2, 1))),
            Net::new(data_net)
                .with_driver(Endpoint::cell(logic, "O"))
                .with_sink(Endpoint::cell(capture, "D"))
                .with_route_segment(RouteSegment::new((2, 1), (3, 1))),
        ],
        clusters: vec![
            Cluster::logic("clb0")
                .with_member(launch)
                .with_capacity(1)
                .at(1, 1),
            Cluster::logic("clb1")
                .with_member(logic)
                .with_capacity(1)
                .at(2, 1),
            Cluster::logic("clb2")
                .with_member(capture)
                .with_capacity(1)
                .at(3, 1),
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
    assert!(
        artifact
            .report_text
            .contains("Longest path delay  : 0.790 ns")
    );
    let path = summary.top_paths.first().expect("critical path");
    assert_eq!(path.startpoint, "in:IN");
    assert_eq!(path.logic_levels, 2);
    assert!(!path.points.is_empty());
    assert!(
        (path.points.last().expect("last timing point").cumulative_ns - path.delay_ns).abs() < 1e-9,
        "path point increments must reconcile to the reported path delay"
    );
    assert!(artifact.report_text.contains("Data arrival time"));
    let timing_json: serde_json::Value =
        serde_json::from_str(&artifact.report_json).expect("timing report json");
    assert_eq!(
        timing_json
            .get("constraint_status")
            .and_then(serde_json::Value::as_str),
        Some("unconstrained")
    );
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
    assert_eq!(delays.len(), 1);
    assert!((delays[0] - 0.79).abs() < 1e-9);

    Ok(())
}

#[test]
fn sta_applies_clock_period_setup_and_clock_to_q() -> Result<()> {
    let design = register_path_design("launch", "logic", "capture", "q_net", "data_net");
    let timing = StaTimingContext {
        clocks: Arc::from([ClockConstraint {
            name: "sys".to_string(),
            port_name: "clk".to_string(),
            period_ns: 2.0,
        }]),
        cell_timing: Some(Arc::new(CellTimingModel {
            sequential: SequentialTiming {
                clock_to_q_ns: 1.0,
                setup_ns: 0.5,
            },
        })),
        ..StaTimingContext::default()
    };

    let options = StaOptions {
        arch: Some(mini_arch().into()),
        delay: None,
    };
    let artifact = run_with_timing(design.clone(), &options, &timing)?.value;

    let summary = artifact.design.timing.expect("timing summary");
    assert!((summary.critical_path_ns - 1.87).abs() < 1e-9);
    let capture = artifact
        .graph
        .nodes
        .iter()
        .find(|node| node.id == "cell:capture:D")
        .expect("capture D timing node");
    assert!((capture.arrival_ns - 1.37).abs() < 1e-9);
    assert!((capture.required_ns - 1.5).abs() < 1e-9);
    assert!((capture.slack_ns - 0.13).abs() < 1e-9);
    assert!(artifact.report_text.contains("Worst Slack: 0.130 ns (MET)"));

    let violating_timing = StaTimingContext {
        clocks: Arc::from([ClockConstraint {
            name: "sys".to_string(),
            port_name: "clk".to_string(),
            period_ns: 1.5,
        }]),
        ..timing
    };
    let violation = run_with_timing(design.clone(), &options, &violating_timing)?.value;
    assert!(
        violation
            .report_text
            .contains("Worst Slack: -0.370 ns (VIOLATED)")
    );

    let multiple_clocks = StaTimingContext {
        clocks: Arc::from([
            ClockConstraint {
                name: "a".to_string(),
                port_name: "clk".to_string(),
                period_ns: 2.0,
            },
            ClockConstraint {
                name: "b".to_string(),
                port_name: "other_clk".to_string(),
                period_ns: 3.0,
            },
        ]),
        cell_timing: None,
        ..StaTimingContext::default()
    };
    let error = run_with_timing(design, &options, &multiple_clocks)
        .expect_err("unknown clock ports must be rejected");
    assert!(matches!(
        error,
        super::StaError::UnknownClockPort { port, .. } if port == "other_clk"
    ));

    Ok(())
}

#[test]
fn sta_text_report_uses_professional_labels_without_yosys_internal_names() -> Result<()> {
    let launch = "$auto$ff.cc:337:slice$1149";
    let logic = "$abc$20704$auto$blifparse.cc:557:parse_blif$20705";
    let capture = "state[3]_DFFHQ_Q";
    let design = register_path_design(launch, logic, capture, "$abc$20704$q", "$abc$20704$data");
    let timing = StaTimingContext {
        clocks: Arc::from([ClockConstraint {
            name: "sys".to_string(),
            port_name: "clk".to_string(),
            period_ns: 2.0,
        }]),
        cell_timing: Some(Arc::new(CellTimingModel {
            sequential: SequentialTiming {
                clock_to_q_ns: 1.0,
                setup_ns: 0.5,
            },
        })),
        ..StaTimingContext::default()
    };

    let artifact = run_with_timing(
        design,
        &StaOptions {
            arch: Some(mini_arch().into()),
            delay: None,
        },
        &timing,
    )?
    .value;
    let report = artifact.report_text;

    for leaked in [
        "$abc$",
        "$auto$",
        "ff.cc",
        "blifparse.cc",
        "proc_rom.cc",
        "rtlil.cc",
        "DFFHQ_Q",
    ] {
        assert!(!report.contains(leaked), "leaked {leaked}:\n{report}");
    }
    for label in ["Register 1/Q", "LUT4 1", "Net 1", "state[3]/D"] {
        assert!(report.contains(label), "missing {label}:\n{report}");
    }
    for section in [
        "Path Type",
        "Launch Clock",
        "Capture Clock",
        "Data Path",
        "Delay Breakdown",
        "Timing Calculation",
        "Clock-to-Q",
        "Cell delay",
        "Net delay",
        "Library setup time",
    ] {
        assert!(report.contains(section), "missing {section}:\n{report}");
    }

    Ok(())
}

#[test]
fn sta_groups_paths_across_multiple_clock_domains() -> Result<()> {
    let design = Design {
        name: "sta-multiclock".to_string(),
        stage: "routed".to_string(),
        ports: vec![
            Port::input("clk_a").at(0, 0),
            Port::input("clk_b").at(0, 1),
            Port::input("din").at(0, 2),
        ],
        cells: vec![
            Cell::ff("launch", "DFFHQ")
                .with_input("D", "din_net")
                .with_input("CK", "clk_a_net")
                .with_output("Q", "q_net")
                .in_cluster("clb0"),
            Cell::ff("capture", "DFFHQ")
                .with_input("D", "q_net")
                .with_input("CK", "clk_b_net")
                .in_cluster("clb1"),
        ],
        nets: vec![
            Net::new("clk_a_net")
                .with_driver(Endpoint::port("clk_a", "IN"))
                .with_sink(Endpoint::cell("launch", "CK")),
            Net::new("clk_b_net")
                .with_driver(Endpoint::port("clk_b", "IN"))
                .with_sink(Endpoint::cell("capture", "CK")),
            Net::new("din_net")
                .with_driver(Endpoint::port("din", "IN"))
                .with_sink(Endpoint::cell("launch", "D")),
            Net::new("q_net")
                .with_driver(Endpoint::cell("launch", "Q"))
                .with_sink(Endpoint::cell("capture", "D")),
        ],
        clusters: vec![
            Cluster::logic("clb0")
                .with_member("launch")
                .with_capacity(1)
                .at(1, 1),
            Cluster::logic("clb1")
                .with_member("capture")
                .with_capacity(1)
                .at(2, 1),
        ],
        ..Design::default()
    };
    let timing = StaTimingContext {
        clocks: Arc::from([
            ClockConstraint {
                name: "a".to_string(),
                port_name: "clk_a".to_string(),
                period_ns: 5.0,
            },
            ClockConstraint {
                name: "b".to_string(),
                port_name: "clk_b".to_string(),
                period_ns: 8.0,
            },
        ]),
        cell_timing: None,
        ..StaTimingContext::default()
    };

    let artifact = run_with_timing(design, &StaOptions::default(), &timing)?.value;
    let summary = artifact.design.timing.expect("timing summary");
    assert_eq!(summary.clocks.len(), 2);
    assert!(summary.path_groups.iter().any(|group| group.name == "a"));
    assert!(summary.path_groups.iter().any(|group| group.name == "b"));
    let crossing = summary
        .top_paths
        .iter()
        .find(|path| path.endpoint == "capture:D")
        .expect("cross-domain path");
    assert_eq!(crossing.launch_clock.as_deref(), Some("a"));
    assert_eq!(crossing.capture_clock.as_deref(), Some("b"));

    Ok(())
}

#[test]
fn sta_applies_sdc_io_delays_uncertainty_and_reports_full_coverage() -> Result<()> {
    let design = Design {
        name: "sta-io-delays".to_string(),
        stage: "routed".to_string(),
        ports: vec![
            Port::input("clk").at(1, 1),
            Port::input("din").at(1, 1),
            Port::output("dout").at(1, 1),
        ],
        cells: vec![
            Cell::ff("reg", "DFFHQ")
                .with_input("D", "din_net")
                .with_input("CK", "clk_net")
                .with_output("Q", "dout_net")
                .in_cluster("clb0"),
        ],
        nets: vec![
            Net::new("clk_net")
                .with_driver(Endpoint::port("clk", "clk"))
                .with_sink(Endpoint::cell("reg", "CK")),
            Net::new("din_net")
                .with_driver(Endpoint::port("din", "din"))
                .with_sink(Endpoint::cell("reg", "D")),
            Net::new("dout_net")
                .with_driver(Endpoint::cell("reg", "Q"))
                .with_sink(Endpoint::port("dout", "dout")),
        ],
        clusters: vec![
            Cluster::logic("clb0")
                .with_member("reg")
                .with_capacity(1)
                .at(1, 1),
        ],
        ..Design::default()
    };
    let timing = StaTimingContext {
        clocks: Arc::from([ClockConstraint {
            name: "sys".to_string(),
            port_name: "clk".to_string(),
            period_ns: 10.0,
        }]),
        input_delays: Arc::from([IoDelayConstraint {
            port_name: "din".to_string(),
            clock_name: "sys".to_string(),
            delay_ns: 2.0,
        }]),
        output_delays: Arc::from([IoDelayConstraint {
            port_name: "dout".to_string(),
            clock_name: "sys".to_string(),
            delay_ns: 2.5,
        }]),
        clock_uncertainties: Arc::from([ClockUncertaintyConstraint {
            clock_name: "sys".to_string(),
            setup_ns: 0.2,
        }]),
        cell_timing: Some(Arc::new(CellTimingModel {
            sequential: SequentialTiming {
                clock_to_q_ns: 1.0,
                setup_ns: 0.5,
            },
        })),
    };

    let artifact = run_with_timing(design, &StaOptions::default(), &timing)?.value;
    let summary = artifact.design.timing.expect("timing summary");

    assert_eq!(
        summary.constraint_status,
        crate::ir::TimingConstraintStatus::Met
    );
    assert_eq!(summary.coverage.constrained_primary_inputs, 1);
    assert_eq!(summary.coverage.constrained_primary_outputs, 1);
    assert!((summary.clocks[0].setup_uncertainty_ns - 0.2).abs() < f64::EPSILON);
    let input_path = summary
        .top_paths
        .iter()
        .find(|path| path.endpoint == "reg:D")
        .expect("input-to-register path");
    assert!(
        (input_path.data_arrival_ns - 2.0).abs() < 1e-9,
        "arrival was {}",
        input_path.data_arrival_ns
    );
    assert!((input_path.data_required_ns.unwrap() - 9.3).abs() < 1e-9);
    let output_path = summary
        .top_paths
        .iter()
        .find(|path| path.endpoint == "dout:dout")
        .expect("register-to-output path");
    assert!((output_path.data_required_ns.unwrap() - 7.3).abs() < 1e-9);

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
        ..Default::default()
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
        ..Default::default()
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
fn sta_rejects_positive_delay_combinational_loops() {
    let design = Design {
        name: "sta-loop".to_string(),
        stage: "routed".to_string(),
        cells: vec![
            Cell::lut("a", "LUT1")
                .with_input("ADR0", "b_to_a")
                .with_output("O", "a_to_b"),
            Cell::lut("b", "LUT1")
                .with_input("ADR0", "a_to_b")
                .with_output("O", "b_to_a"),
        ],
        nets: vec![
            Net::new("a_to_b")
                .with_driver(Endpoint::cell("a", "O"))
                .with_sink(Endpoint::cell("b", "ADR0")),
            Net::new("b_to_a")
                .with_driver(Endpoint::cell("b", "O"))
                .with_sink(Endpoint::cell("a", "ADR0")),
        ],
        ..Design::default()
    };

    let error = run(design, &StaOptions::default()).expect_err("loop must fail");
    assert!(matches!(error, super::StaError::CombinationalLoop { .. }));
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
    assert!(summary.critical_path_ns.abs() < f64::EPSILON);
    assert!(summary.fmax_mhz.abs() < f64::EPSILON);
    assert!(summary.top_paths.is_empty());
    assert!(artifact.graph.nodes.is_empty());
    assert!(artifact.graph.edges.is_empty());
    assert!(
        artifact
            .report_text
            .contains("Longest path delay  : 0.000 ns")
    );

    Ok(())
}

#[test]
fn sta_reporter_receives_stage_events() -> Result<()> {
    struct Collector(Arc<Mutex<Vec<StageEvent>>>);

    impl StageReporter for Collector {
        fn on_stage_event(&mut self, event: StageEvent) {
            self.0.lock().expect("event lock").push(event);
        }
    }

    let events = Arc::new(Mutex::new(Vec::<StageEvent>::new()));
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
    drop(events);

    Ok(())
}

#[test]
fn intrinsic_cell_delay_follows_overridable_cell_delays() {
    use crate::resource::DelayModel;

    let lut = Cell::lut("lut0", "LUT4").with_input("A", "a");

    // Legacy defaults: 0.15 base + 0.04 per input for a single-input LUT.
    let default = super::delay::combinational_cell_delay_ns(&lut, None);
    assert!((default - 0.19).abs() < 1e-9);

    let mut model = DelayModel::default();
    model.cell_delays.lut_base_ns = 0.5;
    model.cell_delays.lut_per_input_ns = 0.01;
    let overridden = super::delay::combinational_cell_delay_ns(&lut, Some(&model));
    assert!((overridden - 0.51).abs() < 1e-9);
}

#[test]
fn lut_timing_arcs_follow_truth_table_dependencies() {
    let mut gate = Cell::lut("gate", "LUT2")
        .with_input("ADR0", "data")
        .with_input("ADR1", "clock");
    gate.set_property("lut_init", "0xA");

    assert!(super::delay::cell_input_is_functional(&gate, "ADR0"));
    assert!(!super::delay::cell_input_is_functional(&gate, "ADR1"));
}

#[test]
fn backward_slack_distinguishes_parallel_branches() -> Result<()> {
    let artifact = run(build_fanin_design(), &StaOptions::default())?.value;

    let graph = &artifact.graph;
    let slack_of = |id: &str| {
        graph
            .nodes
            .iter()
            .find(|node| node.id == id)
            .map_or_else(|| panic!("missing node {id}"), |node| node.slack_ns)
    };

    // Slow branch (b_net, 5 units -> 0.40 arrival + 0.23 cell arc) consumes
    // the whole reference period -> zero slack; the faster a_net branch keeps
    // exactly the skew between the two arrivals as positive slack.
    let slow = slack_of("cell:u0:B");
    let fast = slack_of("cell:u0:A");
    assert!((slow - 0.0).abs() < 1e-9, "slow branch slack {slow}");
    assert!((fast - 0.08).abs() < 1e-9, "fast branch slack {fast}");
    assert!(fast > slow);

    Ok(())
}

fn build_fanin_design() -> Design {
    Design {
        name: "sta-slack".to_string(),
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
    }
}
