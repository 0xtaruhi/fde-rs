use std::path::PathBuf;

use crate::{cil::load_cil, resource::load_arch};

use super::{
    graph::load_site_route_graphs,
    stitch::{clock_spine_neighbors, stitched_neighbors},
    types::RouteNode,
    wire::parse_indexed_wire,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn wire_index_parser_normalizes_edge_and_long_wires() {
    assert_eq!(parse_indexed_wire("E9"), Some(("E".to_string(), 9)));
    assert_eq!(parse_indexed_wire("LEFT_E10"), Some(("E".to_string(), 10)));
    assert_eq!(
        parse_indexed_wire("TOP_LLV7"),
        Some(("TOP_LLV".to_string(), 7))
    );
    assert_eq!(
        parse_indexed_wire("RIGHT_LLH5"),
        Some(("RIGHT_LLH".to_string(), 5))
    );
    assert_eq!(parse_indexed_wire("LLV6"), Some(("LLV".to_string(), 6)));
    assert_eq!(parse_indexed_wire("LLH6"), Some(("LLH".to_string(), 6)));
    assert_eq!(parse_indexed_wire("H6W7"), Some(("H6W".to_string(), 7)));
    assert_eq!(
        parse_indexed_wire("LEFT_H6M10"),
        Some(("H6M".to_string(), 10))
    );
    assert_eq!(parse_indexed_wire("BOT_V6N2"), Some(("V6N".to_string(), 2)));
    assert_eq!(parse_indexed_wire("S0_F_B4"), None);
}

#[test]
fn parses_external_transmission_graphs_when_resources_exist() {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return;
    };
    let arch = bundle.root.join("fdp3p7_arch.xml");
    let cil = bundle.root.join("fdp3p7_cil.xml");
    if !arch.exists() || !cil.exists() {
        return;
    }
    let cil = load_cil(&cil).expect("load cil");
    let graphs = load_site_route_graphs(&arch, &cil).expect("load route graphs");
    let center = graphs.get("GSB_CNT").expect("center graph");
    assert!(
        center
            .adjacency
            .get("W9")
            .is_some_and(|indices| indices.iter().any(|index| center.arcs[*index].to == "E9"))
    );
    assert!(
        center
            .adjacency
            .get("N7")
            .is_some_and(|indices| indices.iter().any(|index| center.arcs[*index].to == "N_P7"))
    );
    let left = graphs.get("GSB_LFT").expect("left graph");
    assert!(left.adjacency.get("LEFT_I1").is_some_and(|indices| {
        indices
            .iter()
            .any(|index| left.arcs[*index].to == "LEFT_E10")
    }));
}

#[test]
fn critical_shift4_exact_pip_pairs_are_unique_in_center_graph() {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return;
    };
    let arch = bundle.root.join("fdp3p7_arch.xml");
    let cil = bundle.root.join("fdp3p7_cil.xml");
    if !arch.exists() || !cil.exists() {
        return;
    }
    let cil = load_cil(&cil).expect("load cil");
    let graphs = load_site_route_graphs(&arch, &cil).expect("load route graphs");
    let center = graphs.get("GSB_CNT").expect("center graph");

    for (from, to) in [
        ("S0_XQ", "OUT2"),
        ("S0_XQ", "OUT3"),
        ("S0_YQ", "OUT2"),
        ("S0_YQ", "OUT4"),
        ("S0_YQ", "OUT5"),
        ("OUT2", "LLV0"),
        ("OUT2", "N6"),
        ("OUT3", "V6N9"),
        ("OUT4", "S13"),
        ("OUT5", "W17"),
        ("LLV0", "V6N1"),
        ("LLV6", "V6N2"),
    ] {
        let matches = center
            .adjacency
            .get(from)
            .into_iter()
            .flat_map(|indices| indices.iter())
            .filter(|index| {
                center
                    .arcs
                    .get(**index)
                    .is_some_and(|arc| arc.from == from && arc.to == to)
            })
            .count();
        assert_eq!(matches, 1, "expected unique arc for {from} -> {to}");
    }
}

#[test]
fn clock_spine_stitching_reaches_center_and_clkv_tiles() {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return;
    };
    let arch = bundle.root.join("fdp3p7_arch.xml");
    if !arch.exists() {
        return;
    }
    let arch = load_arch(&arch).expect("load arch");

    let from_clkb = RouteNode {
        x: 34,
        y: 27,
        net: "CLKB_GCLK0".to_string(),
    };
    let clkb_neighbors = clock_spine_neighbors(&arch, &from_clkb);
    assert!(clkb_neighbors.contains(&(17, 27, "CLKC_GCLK0".to_string())));

    let from_clkc = RouteNode {
        x: 17,
        y: 27,
        net: "CLKC_VGCLK0".to_string(),
    };
    let clkc_neighbors = clock_spine_neighbors(&arch, &from_clkc);
    assert!(clkc_neighbors.contains(&(16, 27, "CLKV_VGCLK0".to_string())));

    let from_clkv = RouteNode {
        x: 16,
        y: 27,
        net: "CLKV_GCLK_BUFL0".to_string(),
    };
    let clkv_neighbors = clock_spine_neighbors(&arch, &from_clkv);
    assert!(clkv_neighbors.contains(&(16, 26, "GCLK0".to_string())));
}

#[test]
fn clock_spine_stitching_reaches_edge_backbones_for_global_clock_lut_routes() {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return;
    };
    let arch = bundle.root.join("fdp3p7_arch.xml");
    if !arch.exists() {
        return;
    }
    let arch = load_arch(&arch).expect("load arch");

    let from_clkb_llh1 = RouteNode {
        x: 34,
        y: 27,
        net: "CLKB_LLH1".to_string(),
    };
    let clkb_llh1_neighbors = clock_spine_neighbors(&arch, &from_clkb_llh1);
    assert!(clkb_llh1_neighbors.contains(&(34, 8, "BOT_LLH6".to_string())));

    let from_clkb_llh4 = RouteNode {
        x: 34,
        y: 27,
        net: "CLKB_LLH4".to_string(),
    };
    let clkb_llh4_neighbors = clock_spine_neighbors(&arch, &from_clkb_llh4);
    assert!(clkb_llh4_neighbors.contains(&(34, 1, "LL_LLH4".to_string())));

    let from_ll_h6b5 = RouteNode {
        x: 34,
        y: 1,
        net: "LL_H6B5".to_string(),
    };
    let ll_h6b5_neighbors = clock_spine_neighbors(&arch, &from_ll_h6b5);
    assert!(ll_h6b5_neighbors.contains(&(34, 3, "BOT_H6C5".to_string())));

    let from_bot_v6a7 = RouteNode {
        x: 34,
        y: 8,
        net: "BOT_V6A7".to_string(),
    };
    let bot_v6a7_neighbors = clock_spine_neighbors(&arch, &from_bot_v6a7);
    assert!(bot_v6a7_neighbors.contains(&(32, 8, "V6M7".to_string())));

    let from_bot_llv2 = RouteNode {
        x: 34,
        y: 3,
        net: "BOT_LLV2".to_string(),
    };
    let bot_llv2_neighbors = stitched_neighbors(&arch, &from_bot_llv2);
    assert!(bot_llv2_neighbors.iter().any(|(x, y, net)| {
        *y == 3
            && matches!(net.as_str(), "LLV0" | "LLV6")
            && arch
                .tile_at(*x, *y)
                .is_some_and(|tile| tile.tile_type == "CENTER")
    }));
}

#[test]
fn llh_stitching_connects_edge_longlines_to_center_row_and_back() {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return;
    };
    let arch = bundle.root.join("fdp3p7_arch.xml");
    if !arch.exists() {
        return;
    }
    let arch = load_arch(&arch).expect("load arch");

    let from_right = RouteNode {
        x: 3,
        y: 53,
        net: "RIGHT_LLH7".to_string(),
    };
    let right_neighbors = stitched_neighbors(&arch, &from_right);
    assert!(right_neighbors.contains(&(3, 13, "LLH0".to_string())));
    assert!(right_neighbors.contains(&(3, 13, "LLH6".to_string())));

    let from_center = RouteNode {
        x: 4,
        y: 13,
        net: "LLH0".to_string(),
    };
    let center_neighbors = stitched_neighbors(&arch, &from_center);
    assert!(center_neighbors.contains(&(4, 1, "LEFT_LLH0".to_string())));
    assert!(center_neighbors.contains(&(4, 53, "RIGHT_LLH0".to_string())));
}

#[test]
fn llv_stitching_connects_edge_longlines_to_center_column_and_back() {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return;
    };
    let arch = bundle.root.join("fdp3p7_arch.xml");
    if !arch.exists() {
        return;
    }
    let arch = load_arch(&arch).expect("load arch");

    let from_top = RouteNode {
        x: 0,
        y: 2,
        net: "TOP_LLV7".to_string(),
    };
    let top_neighbors = stitched_neighbors(&arch, &from_top);
    assert!(top_neighbors.iter().any(|(x, y, net)| {
        *y == 2
            && matches!(net.as_str(), "LLV0" | "LLV6")
            && arch
                .tile_at(*x, *y)
                .is_some_and(|tile| tile.tile_type == "CENTER")
    }));

    let center_x = arch
        .tiles
        .values()
        .filter(|tile| tile.logic_y == 2 && tile.tile_type == "CENTER")
        .map(|tile| tile.logic_x)
        .min()
        .expect("center tile in LLV column");
    let from_center = RouteNode {
        x: center_x,
        y: 2,
        net: "LLV0".to_string(),
    };
    let center_neighbors = stitched_neighbors(&arch, &from_center);
    assert!(center_neighbors.iter().any(|(x, y, net)| {
        *y == 2
            && *x != center_x
            && matches!(net.as_str(), "LLV0" | "LLV6")
            && arch
                .tile_at(*x, *y)
                .is_some_and(|tile| tile.tile_type == "CENTER")
    }));
    assert!(center_neighbors.contains(&(0, 2, "TOP_LLV0".to_string())));
}

#[test]
fn h6m_stitching_connects_mark_style_center_path() {
    let Some(bundle) = crate::resource::ResourceBundle::discover_from(&repo_root()).ok() else {
        return;
    };
    let arch = bundle.root.join("fdp3p7_arch.xml");
    if !arch.exists() {
        return;
    }
    let arch = load_arch(&arch).expect("load arch");

    let from_h6w = RouteNode {
        x: 2,
        y: 5,
        net: "H6W10".to_string(),
    };
    let from_h6w_neighbors = stitched_neighbors(&arch, &from_h6w);
    assert!(
        from_h6w_neighbors.contains(&(2, 2, "H6M10".to_string())),
        "expected mark-style H6W -> H6M stitch near the left edge"
    );

    let from_h6m = RouteNode {
        x: 2,
        y: 2,
        net: "H6M10".to_string(),
    };
    let from_h6m_neighbors = stitched_neighbors(&arch, &from_h6m);
    assert!(from_h6m_neighbors.contains(&(2, 5, "H6W10".to_string())));
}
