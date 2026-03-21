use super::{
    DeviceCell, DeviceDesign, DeviceEndpoint, DeviceNet, DeviceSinkGuide,
    annotate_exact_route_pips, lower_design,
};
use crate::{
    bitgen::{BitgenOptions, run as run_bitgen},
    cil::load_cil,
    constraints::load_constraints,
    ir::{Cell, CellPin, Cluster, Design, Endpoint, Net, RoutePip, RouteSegment},
    map::{MapOptions, load_input, run as run_map},
    pack::{PackOptions, run as run_pack},
    place::{PlaceMode, PlaceOptions, run as run_place},
    resource::{load_arch, load_delay_model},
    route_bits::route_device_design,
};
use anyhow::Result;
use std::{collections::BTreeMap, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn segments_from_points(points: &[(usize, usize)]) -> Vec<RouteSegment> {
    points
        .windows(2)
        .map(|window| RouteSegment {
            x0: window[0].0,
            y0: window[0].1,
            x1: window[1].0,
            y1: window[1].1,
        })
        .collect()
}

fn guided_logic_design(
    src: (usize, usize),
    dst: (usize, usize),
    guide_points: &[(usize, usize)],
) -> Design {
    Design {
        name: "guided-device-route".to_string(),
        stage: "routed".to_string(),
        cells: vec![
            Cell {
                name: "src".to_string(),
                kind: "lut".to_string(),
                type_name: "LUT4".to_string(),
                outputs: vec![CellPin {
                    port: "O".to_string(),
                    net: "link".to_string(),
                }],
                cluster: Some("clb_src".to_string()),
                ..Cell::default()
            },
            Cell {
                name: "dst".to_string(),
                kind: "lut".to_string(),
                type_name: "LUT4".to_string(),
                inputs: vec![CellPin {
                    port: "ADR0".to_string(),
                    net: "link".to_string(),
                }],
                cluster: Some("clb_dst".to_string()),
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
                pin: "ADR0".to_string(),
            }],
            route: segments_from_points(guide_points),
            ..Net::default()
        }],
        clusters: vec![
            Cluster {
                name: "clb_src".to_string(),
                kind: "logic".to_string(),
                members: vec!["src".to_string()],
                capacity: 1,
                x: Some(src.0),
                y: Some(src.1),
                ..Cluster::default()
            },
            Cluster {
                name: "clb_dst".to_string(),
                kind: "logic".to_string(),
                members: vec!["dst".to_string()],
                capacity: 1,
                x: Some(dst.0),
                y: Some(dst.1),
                ..Cluster::default()
            },
        ],
        ..Design::default()
    }
}

type GuidedPair = (
    (usize, usize),
    (usize, usize),
    Vec<(usize, usize)>,
    Vec<(usize, usize)>,
);

fn find_guided_pair(arch: &crate::resource::Arch) -> Option<GuidedPair> {
    guided_pairs(arch).into_iter().next()
}

fn guided_pairs(arch: &crate::resource::Arch) -> Vec<GuidedPair> {
    let sites = arch.logic_sites();
    let mut pairs = Vec::new();
    for &src in &sites {
        for &dst in &sites {
            if src == dst || src.0 != dst.0 || src.1.abs_diff(dst.1) < 4 {
                continue;
            }
            let (low_y, high_y) = if src.1 < dst.1 {
                (src.1, dst.1)
            } else {
                (dst.1, src.1)
            };
            for detour_x in [src.0.saturating_add(1), src.0.saturating_sub(1)] {
                if detour_x == src.0 || detour_x >= arch.width {
                    continue;
                }
                let direct = (low_y..=high_y).map(|y| (src.0, y)).collect::<Vec<_>>();
                let mut detour = Vec::new();
                detour.push((src.0, low_y));
                detour.push((detour_x, low_y));
                detour.extend((low_y + 1..=high_y).map(|y| (detour_x, y)));
                detour.push((src.0, high_y));
                if direct
                    .iter()
                    .chain(detour.iter())
                    .all(|&(x, y)| arch.tile_at(x, y).is_some())
                {
                    let (ordered_src, ordered_dst) = if src.1 <= dst.1 {
                        (src, dst)
                    } else {
                        (dst, src)
                    };
                    pairs.push((ordered_src, ordered_dst, direct, detour));
                }
            }
        }
    }
    pairs
}

fn device_endpoint(name: &str, pin: &str, x: usize, y: usize, z: usize) -> DeviceEndpoint {
    DeviceEndpoint {
        kind: "cell".to_string(),
        name: name.to_string(),
        pin: pin.to_string(),
        x,
        y,
        z,
    }
}

fn guided_clock_to_lut_device_design() -> DeviceDesign {
    let sink = device_endpoint("lut_sink", "ADR1", 32, 9, 0);
    DeviceDesign {
        name: "guided-gclk-to-lut".to_string(),
        device: "fdp3p7".to_string(),
        cells: vec![
            DeviceCell {
                cell_name: "$gclk$clk".to_string(),
                type_name: "GCLK".to_string(),
                site_kind: "GCLK".to_string(),
                site_name: "GCLKBUF1".to_string(),
                bel: "BUF".to_string(),
                tile_name: "BM".to_string(),
                tile_type: "CLKB".to_string(),
                x: 34,
                y: 27,
                z: 1,
                synthetic: true,
                ..DeviceCell::default()
            },
            DeviceCell {
                cell_name: "lut_sink".to_string(),
                type_name: "LUT2".to_string(),
                site_kind: "SLICE".to_string(),
                site_name: "SLICE0".to_string(),
                bel: "BEL0".to_string(),
                tile_name: "R31C8".to_string(),
                tile_type: "CENTER".to_string(),
                x: 32,
                y: 9,
                z: 0,
                ..DeviceCell::default()
            },
        ],
        nets: vec![DeviceNet {
            name: "clk".to_string(),
            driver: Some(device_endpoint("$gclk$clk", "OUT", 34, 27, 1)),
            sinks: vec![sink.clone()],
            origin: "logical-net".to_string(),
            route_pips: Vec::new(),
            guide_tiles: vec![(34, 27), (34, 8), (32, 8), (32, 9)],
            sink_guides: vec![DeviceSinkGuide {
                sink,
                tiles: vec![(34, 27), (34, 8), (32, 8), (32, 9)],
            }],
        }],
        ..DeviceDesign::default()
    }
}

fn parallel_slice_outputs_device_design() -> DeviceDesign {
    let sink_p7 = device_endpoint("out_p7", "OUT", 5, 1, 2);
    let sink_p6 = device_endpoint("out_p6", "OUT", 5, 1, 1);
    let guide = (1..=10).rev().map(|y| (5, y)).collect::<Vec<_>>();
    DeviceDesign {
        name: "parallel-slice-outputs".to_string(),
        device: "fdp3p7".to_string(),
        cells: vec![
            DeviceCell {
                cell_name: "src_x".to_string(),
                type_name: "DFFHQ".to_string(),
                site_kind: "SLICE".to_string(),
                site_name: "SLICE0".to_string(),
                bel: "FF0".to_string(),
                tile_name: "R5C9".to_string(),
                tile_type: "CENTER".to_string(),
                x: 5,
                y: 10,
                z: 0,
                ..DeviceCell::default()
            },
            DeviceCell {
                cell_name: "src_y".to_string(),
                type_name: "DFFHQ".to_string(),
                site_kind: "SLICE".to_string(),
                site_name: "SLICE0".to_string(),
                bel: "FF1".to_string(),
                tile_name: "R5C9".to_string(),
                tile_type: "CENTER".to_string(),
                x: 5,
                y: 10,
                z: 0,
                ..DeviceCell::default()
            },
            DeviceCell {
                cell_name: "out_p7".to_string(),
                type_name: "IOB".to_string(),
                site_kind: "IOB".to_string(),
                site_name: "IOB2".to_string(),
                bel: "IO".to_string(),
                tile_name: "LR5".to_string(),
                tile_type: "LEFT".to_string(),
                x: 5,
                y: 1,
                z: 2,
                synthetic: true,
                ..DeviceCell::default()
            },
            DeviceCell {
                cell_name: "out_p6".to_string(),
                type_name: "IOB".to_string(),
                site_kind: "IOB".to_string(),
                site_name: "IOB1".to_string(),
                bel: "IO".to_string(),
                tile_name: "LR5".to_string(),
                tile_type: "LEFT".to_string(),
                x: 5,
                y: 1,
                z: 1,
                synthetic: true,
                ..DeviceCell::default()
            },
        ],
        nets: vec![
            DeviceNet {
                name: "q1".to_string(),
                driver: Some(device_endpoint("src_x", "Q", 5, 10, 0)),
                sinks: vec![sink_p7.clone()],
                origin: "logical-net".to_string(),
                route_pips: Vec::new(),
                guide_tiles: guide.clone(),
                sink_guides: vec![DeviceSinkGuide {
                    sink: sink_p7,
                    tiles: guide.clone(),
                }],
            },
            DeviceNet {
                name: "q2".to_string(),
                driver: Some(device_endpoint("src_y", "Q", 5, 10, 0)),
                sinks: vec![sink_p6.clone()],
                origin: "logical-net".to_string(),
                route_pips: Vec::new(),
                guide_tiles: guide.clone(),
                sink_guides: vec![DeviceSinkGuide {
                    sink: sink_p6,
                    tiles: guide,
                }],
            },
        ],
        ..DeviceDesign::default()
    }
}

#[test]
fn lowering_materializes_clock_and_io_sites_when_external_resources_are_available() -> Result<()> {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return Ok(());
    };
    let arch_path = bundle.root.join("fdp3p7_arch.xml");
    let cil_path = bundle.root.join("fdp3p7_cil.xml");
    let delay_path = bundle.root.join("fdp3p7_dly.xml");
    if !arch_path.exists() || !cil_path.exists() {
        return Ok(());
    }

    let design = load_input(&repo_root().join("tests/fixtures/blinky-yosys.edf"))?;
    let mapped = run_map(design, &MapOptions::default())?.value.design;
    let packed = run_pack(
        mapped,
        &PackOptions {
            family: Some("fdp3".to_string()),
            ..PackOptions::default()
        },
    )?
    .value;
    let arch = load_arch(&arch_path)?;
    let delay = load_delay_model(Some(&delay_path))?;
    let constraints = load_constraints(&repo_root().join("tests/fixtures/fdp3p7-constraints.xml"))?;
    let placed = run_place(
        packed,
        &PlaceOptions {
            arch: arch.clone(),
            delay,
            constraints: constraints.clone(),
            mode: PlaceMode::TimingDriven,
            seed: 0xFDE_2024,
        },
    )?
    .value;
    let cil = load_cil(&cil_path)?;
    let lowered = lower_design(placed, &arch, Some(&cil), &constraints)?;

    assert!(lowered.ports.iter().any(|port| {
        port.port_name == "clk"
            && port.site_kind == "GCLKIOB"
            && port.tile_type == "CLKB"
            && port.site_name == "GCLKIOB0"
    }));
    assert!(lowered.cells.iter().any(|cell| {
        cell.synthetic
            && cell.type_name == "GCLK"
            && cell.cell_name == "$gclk$clk"
            && cell.site_name == "GCLKBUF0"
    }));
    assert!(
        lowered
            .cells
            .iter()
            .any(|cell| !cell.synthetic && cell.type_name == "LUT2" && cell.site_kind == "SLICE")
    );
    assert!(
        lowered
            .nets
            .iter()
            .any(|net| net.origin == "synthetic-gclk")
    );

    Ok(())
}

#[test]
fn lowering_preserves_branch_specific_route_guides_when_resources_are_available() -> Result<()> {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return Ok(());
    };
    let arch_path = bundle.root.join("fdp3p7_arch.xml");
    let cil_path = bundle.root.join("fdp3p7_cil.xml");
    if !arch_path.exists() || !cil_path.exists() {
        return Ok(());
    }

    let arch = load_arch(&arch_path)?;
    let cil = load_cil(&cil_path)?;
    let Some((src, dst, direct_guide, detour_guide)) = find_guided_pair(&arch) else {
        return Ok(());
    };
    let design_direct = guided_logic_design(src, dst, &direct_guide);
    let design_detour = guided_logic_design(src, dst, &detour_guide);
    let lowered_direct = lower_design(design_direct, &arch, Some(&cil), &[])?;
    let lowered_detour = lower_design(design_detour, &arch, Some(&cil), &[])?;

    assert_ne!(
        lowered_direct.nets[0].guide_tiles,
        lowered_detour.nets[0].guide_tiles
    );
    assert_ne!(
        lowered_direct.nets[0].sink_guides[0].tiles,
        lowered_detour.nets[0].sink_guides[0].tiles
    );

    let route_direct = route_device_design(&lowered_direct, &arch, &arch_path, &cil)?;
    let route_detour = route_device_design(&lowered_detour, &arch, &arch_path, &cil)?;
    assert!(!route_direct.pips.is_empty());
    assert!(!route_detour.pips.is_empty());

    let bitstream_direct = run_bitgen(
        guided_logic_design(src, dst, &direct_guide),
        &BitgenOptions {
            arch_name: Some(arch.name.clone()),
            arch_path: Some(arch_path.clone()),
            cil_path: Some(cil_path.clone()),
            cil: Some(cil.clone()),
            device_design: Some(lowered_direct),
        },
    )?
    .value;
    let bitstream_detour = run_bitgen(
        guided_logic_design(src, dst, &detour_guide),
        &BitgenOptions {
            arch_name: Some(arch.name.clone()),
            arch_path: Some(arch_path),
            cil_path: Some(cil_path),
            cil: Some(cil),
            device_design: Some(lowered_detour),
        },
    )?
    .value;

    assert!(!bitstream_direct.bytes.is_empty());
    assert!(!bitstream_detour.bytes.is_empty());
    Ok(())
}

#[test]
fn lowering_preserves_exact_route_pips_when_present() -> Result<()> {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return Ok(());
    };
    let arch_path = bundle.root.join("fdp3p7_arch.xml");
    let cil_path = bundle.root.join("fdp3p7_cil.xml");
    if !arch_path.exists() || !cil_path.exists() {
        return Ok(());
    }

    let arch = load_arch(&arch_path)?;
    let cil = load_cil(&cil_path)?;
    let Some((src, dst, guide, _)) = find_guided_pair(&arch) else {
        return Ok(());
    };
    let mut design = guided_logic_design(src, dst, &guide);
    design.nets[0].route_pips = vec![RoutePip {
        x: src.0,
        y: src.1,
        from_net: "S0_Y".to_string(),
        to_net: "OUT0".to_string(),
        dir: "->".to_string(),
    }];

    let lowered = lower_design(design, &arch, Some(&cil), &[])?;
    assert_eq!(lowered.nets[0].route_pips.len(), 1);
    assert_eq!(lowered.nets[0].route_pips[0].from_net, "S0_Y");
    assert_eq!(lowered.nets[0].route_pips[0].to_net, "OUT0");
    Ok(())
}

#[test]
fn exact_route_annotation_backfills_design_and_device_nets() -> Result<()> {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return Ok(());
    };
    let arch_path = bundle.root.join("fdp3p7_arch.xml");
    let cil_path = bundle.root.join("fdp3p7_cil.xml");
    if !arch_path.exists() || !cil_path.exists() {
        return Ok(());
    }

    let arch = load_arch(&arch_path)?;
    let cil = load_cil(&cil_path)?;
    let Some((src, dst, guide, _)) = find_guided_pair(&arch) else {
        return Ok(());
    };
    let exact = annotate_exact_route_pips(
        guided_logic_design(src, dst, &guide),
        &arch,
        &arch_path,
        &cil,
        &[],
    )?;

    assert!(!exact.route_image.pips.is_empty());
    assert!(!exact.design.nets[0].route_pips.is_empty());
    let device_net = exact
        .device
        .nets
        .iter()
        .find(|net| net.name == exact.design.nets[0].name)
        .expect("device net for logical link");
    assert_eq!(device_net.route_pips, exact.design.nets[0].route_pips);
    Ok(())
}

#[test]
fn device_router_routes_global_clock_guides_into_lut_inputs_when_resources_are_available()
-> Result<()> {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return Ok(());
    };
    let arch_path = bundle.root.join("fdp3p7_arch.xml");
    let cil_path = bundle.root.join("fdp3p7_cil.xml");
    if !arch_path.exists() || !cil_path.exists() {
        return Ok(());
    }

    let arch = load_arch(&arch_path)?;
    let cil = load_cil(&cil_path)?;
    let route = route_device_design(
        &guided_clock_to_lut_device_design(),
        &arch,
        &arch_path,
        &cil,
    )?;

    assert!(
        route
            .notes
            .iter()
            .all(|note| !note.contains("could not find a Rust route"))
    );
    assert!(route.pips.iter().any(|pip| pip.net_name == "clk"
        && pip.from_net == "CLKB_GCLK1_PW"
        && pip.to_net == "CLKB_LLH1"));
    assert!(
        route
            .pips
            .iter()
            .any(|pip| pip.net_name == "clk" && pip.to_net == "S0_F_B2")
    );

    Ok(())
}

#[test]
fn device_router_prefers_exact_route_pips_over_guide_search() -> Result<()> {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return Ok(());
    };
    let arch_path = bundle.root.join("fdp3p7_arch.xml");
    let cil_path = bundle.root.join("fdp3p7_cil.xml");
    if !arch_path.exists() || !cil_path.exists() {
        return Ok(());
    }

    let arch = load_arch(&arch_path)?;
    let cil = load_cil(&cil_path)?;
    let mut device = guided_clock_to_lut_device_design();
    let expected = route_device_design(&device, &arch, &arch_path, &cil)?;
    device.nets[0].guide_tiles = vec![(0, 0)];
    device.nets[0].sink_guides[0].tiles = vec![(0, 0)];
    device.nets[0].route_pips = expected
        .pips
        .iter()
        .filter(|pip| pip.net_name == "clk")
        .map(|pip| RoutePip {
            x: pip.x,
            y: pip.y,
            from_net: pip.from_net.clone(),
            to_net: pip.to_net.clone(),
            dir: "->".to_string(),
        })
        .collect();

    let exact = route_device_design(&device, &arch, &arch_path, &cil)?;
    let expected_tuples = expected
        .pips
        .iter()
        .map(|pip| (pip.x, pip.y, pip.from_net.as_str(), pip.to_net.as_str()))
        .collect::<Vec<_>>();
    let exact_tuples = exact
        .pips
        .iter()
        .map(|pip| (pip.x, pip.y, pip.from_net.as_str(), pip.to_net.as_str()))
        .collect::<Vec<_>>();

    assert_eq!(exact_tuples, expected_tuples);
    assert!(
        exact
            .notes
            .iter()
            .any(|note| note.contains("used") && note.contains("exact routed pip"))
    );
    Ok(())
}

#[test]
fn device_router_keeps_distinct_nets_off_shared_physical_nodes() -> Result<()> {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return Ok(());
    };
    let arch_path = bundle.root.join("fdp3p7_arch.xml");
    let cil_path = bundle.root.join("fdp3p7_cil.xml");
    if !arch_path.exists() || !cil_path.exists() {
        return Ok(());
    }

    let arch = load_arch(&arch_path)?;
    let cil = load_cil(&cil_path)?;
    let route = route_device_design(
        &parallel_slice_outputs_device_design(),
        &arch,
        &arch_path,
        &cil,
    )?;

    assert!(
        route
            .notes
            .iter()
            .all(|note| !note.contains("could not find a Rust route"))
    );
    assert!(
        route
            .pips
            .iter()
            .any(|pip| pip.net_name == "q1" && pip.to_net == "LEFT_O2")
    );
    assert!(
        route
            .pips
            .iter()
            .any(|pip| pip.net_name == "q2" && pip.to_net == "LEFT_O1")
    );
    assert!(
        route
            .pips
            .iter()
            .all(|pip| !(pip.from_net == "OUT5" && pip.to_net == "LLH6"))
    );
    assert!(route.pips.iter().all(|pip| {
        !matches!(pip.from_net.as_str(), "S0_XQ" | "S0_YQ")
            || matches!(
                pip.to_net.as_str(),
                "OUT2" | "OUT3" | "OUT4" | "OUT5" | "OUT6" | "OUT7"
            )
    }));

    let mut owners = BTreeMap::<(usize, usize, String), String>::new();
    for pip in &route.pips {
        let key = (pip.x, pip.y, pip.to_net.clone());
        if let Some(previous) = owners.insert(key.clone(), pip.net_name.clone()) {
            assert_eq!(
                previous, pip.net_name,
                "distinct nets reused physical node {:?}",
                key
            );
        }
    }

    Ok(())
}
