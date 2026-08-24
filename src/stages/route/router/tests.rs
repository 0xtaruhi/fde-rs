use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::path::PathBuf;

use crate::{
    cil::load_cil,
    domain::NetOrigin,
    resource::{
        ResourceBundle, load_arch,
        routing::{
            StitchedComponentDb, build_stitched_components, load_site_route_graphs,
            load_tile_stitch_db,
        },
    },
};

use super::super::types::{RouteNode, SearchState, WireInterner};
use super::{SinkRouteSpec, ordered_start_nodes};
use crate::route::guide::GuideDistances;
use crate::route::{
    guide::OrderedGuide,
    heap::{frontier_heap_pop, frontier_heap_push},
    occupancy::{ClaimIndex, NegotiationContext, RouteResource, reserve_route_path},
};

#[test]
fn ordered_guide_allows_long_span_progress_along_straight_runs() {
    let guide = OrderedGuide::new(&[
        (16, 31),
        (16, 30),
        (16, 29),
        (16, 28),
        (16, 27),
        (16, 26),
        (16, 25),
    ]);
    assert_eq!(guide.advance(0, (16, 31), (16, 25)), Some(6));
    assert_eq!(guide.advance(0, (16, 31), (16, 29)), Some(2));
    assert_eq!(guide.advance(2, (16, 29), (16, 25)), Some(6));
}

#[test]
fn ordered_guide_rejects_skipping_across_turns() {
    let guide = OrderedGuide::new(&[(3, 3), (3, 4), (4, 4), (5, 4)]);
    assert_eq!(guide.advance(0, (3, 3), (5, 4)), None);
    assert_eq!(guide.advance(1, (3, 4), (5, 4)), Some(3));
}

#[test]
fn same_net_resource_revisits_stay_free_of_contention() {
    let stitched_components = StitchedComponentDb::default();
    let mut wires = WireInterner::default();
    let track = RouteNode::new(5, 7, wires.intern("H6W2"));
    let mut claims = ClaimIndex::new(1);

    // The owning net may re-enter its own resource arbitrarily often:
    // shared multi-sink trees legitimately revisit branches.
    for _ in 0..3 {
        reserve_route_path(
            &stitched_components,
            &mut claims,
            0,
            NetOrigin::Logical,
            &[track],
            &[],
        );
    }
    assert_eq!(claims.contested_resources().count(), 0);
    assert_eq!(claims.overuse_count(), 0);
}

#[test]
fn foreign_net_claims_are_unique_and_priced() {
    let stitched_components = StitchedComponentDb::default();
    let mut wires = WireInterner::default();
    let track = RouteNode::new(5, 7, wires.intern("H6W2"));
    let key = stitched_components.occupancy_key(&track);
    let resource = RouteResource::Node(key);
    let mut claims = ClaimIndex::new(4);
    for net in [0, 1, 1, 2] {
        reserve_route_path(
            &stitched_components,
            &mut claims,
            net,
            NetOrigin::Logical,
            &[track],
            &[],
        );
    }
    assert_eq!(claims.contested_resources().count(), 1);
    assert_eq!(claims.overuse_count(), 2);
    assert_eq!(
        claims.claimant_nets(resource).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );

    let history = std::iter::once((resource, 3)).collect();
    let negotiation = NegotiationContext {
        claims: &claims,
        history: &history,
        present_factor: 4,
        net_index: 3,
        net_origin: NetOrigin::Logical,
        hard_block: false,
    };
    assert_eq!(negotiation.penalty(resource, None), 3 + 4 * 3);
}

#[test]
fn ripping_up_the_first_claimant_rebuilds_contention_from_remaining_nets() {
    let stitched_components = StitchedComponentDb::default();
    let mut wires = WireInterner::default();
    let track = RouteNode::new(5, 7, wires.intern("H6W2"));
    let resource = RouteResource::Node(stitched_components.occupancy_key(&track));
    let mut claims = ClaimIndex::new(3);
    for net in 0..3 {
        reserve_route_path(
            &stitched_components,
            &mut claims,
            net,
            NetOrigin::Logical,
            &[track],
            &[],
        );
    }

    claims.rip_up(0);
    assert_eq!(
        claims.claimant_nets(resource).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(claims.overuse_count(), 1);

    claims.rip_up(2);
    assert_eq!(claims.contested_resources().count(), 0);
}

#[test]
fn real_arch_stitched_component_claims_are_contested_across_tiles() {
    let Some(bundle) =
        ResourceBundle::discover_from(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))).ok()
    else {
        return;
    };
    let arch_path = bundle.root.join("fdp3p7_arch.xml");
    if !arch_path.exists() {
        return;
    }

    let arch = load_arch(&arch_path).expect("load arch");
    let mut wires = WireInterner::default();
    let db = load_tile_stitch_db(&arch_path, &mut wires).expect("load stitch db");
    let components = build_stitched_components(&db, &arch, &wires);

    let upper = RouteNode::new(4, 53, wires.intern("RIGHT_LLH3"));
    let lower = RouteNode::new(4, 5, wires.intern("LLH0"));
    let tree_nodes: HashSet<RouteNode> = HashSet::default();

    assert_eq!(
        components.occupancy_key(&upper),
        components.occupancy_key(&lower)
    );

    let mut claims = ClaimIndex::new(3);
    for (node, net) in [(&upper, 1usize), (&lower, 2usize)] {
        reserve_route_path(
            &components,
            &mut claims,
            net,
            NetOrigin::Logical,
            &[*node],
            &[],
        );
    }
    let _ = tree_nodes;

    assert_eq!(claims.contested_resources().count(), 1);
    assert_eq!(claims.overuse_count(), 1);
}

#[test]
fn frontier_heap_has_stable_equal_cost_pop_order() {
    let mut wires = WireInterner::default();
    let mut heap = Vec::new();
    for node in 0..8 {
        frontier_heap_push(
            &mut heap,
            SearchState {
                cost: 1,
                priority: 1,
                order: 0,
                key: wires.intern_indexed("N", node),
                node: RouteNode::new(0, 0, wires.intern_indexed("N", node)),
            },
        );
    }

    let mut popped = Vec::new();
    while let Some(state) = frontier_heap_pop(&mut heap) {
        popped.push(state.node.wire);
    }

    let expected = [0usize, 1, 3, 7, 6, 5, 4, 2]
        .into_iter()
        .map(|index| wires.intern_indexed("N", index))
        .collect::<Vec<_>>();
    assert_eq!(popped, expected);
}

#[test]
fn ordered_start_nodes_prioritize_existing_tree_frontier() {
    let mut wires = WireInterner::default();
    let root = RouteNode::new(0, 0, wires.intern("ROOT"));
    let near_tree = RouteNode::new(4, 4, wires.intern("NEAR"));
    let far_tree = RouteNode::new(1, 1, wires.intern("FAR"));
    let ordered_guide = OrderedGuide::new(&[]);
    let guide_distances = GuideDistances::new(&crate::resource::Arch::default(), &[]);
    let tree_nodes = [root, far_tree, near_tree]
        .into_iter()
        .collect::<HashSet<_>>();
    let tree_starts = tree_nodes.clone();
    let tree_start_costs = [(root, 0usize), (far_tree, 0usize), (near_tree, 0usize)]
        .into_iter()
        .collect::<HashMap<_, _>>();
    let sink_wires = [wires.intern("SINK")];
    let spec = SinkRouteSpec {
        criticality: 0.0,
        net_kind: super::RouteNetKind::Generic,
        strict_clock_sink: false,
        ordered_guide: &ordered_guide,
        guide_distances: &guide_distances,
        roots: &[root],
        tree_nodes: &tree_nodes,
        tree_starts: &tree_starts,
        tree_start_costs: &tree_start_costs,
        sink_x: 5,
        sink_y: 5,
        sink_wires: &sink_wires,
    };

    assert_eq!(
        ordered_start_nodes(&spec).into_vec(),
        vec![near_tree, far_tree, root]
    );
}

#[test]
fn ordered_start_nodes_prefer_lower_tree_cost_over_nearer_frontier() {
    let mut wires = WireInterner::default();
    let root = RouteNode::new(0, 0, wires.intern("ROOT"));
    let near_tree = RouteNode::new(4, 4, wires.intern("NEAR"));
    let far_tree = RouteNode::new(1, 1, wires.intern("FAR"));
    let ordered_guide = OrderedGuide::new(&[]);
    let guide_distances = GuideDistances::new(&crate::resource::Arch::default(), &[]);
    let tree_nodes = [root, far_tree, near_tree]
        .into_iter()
        .collect::<HashSet<_>>();
    let tree_starts = tree_nodes.clone();
    let tree_start_costs = [(root, 0usize), (far_tree, 1usize), (near_tree, 5usize)]
        .into_iter()
        .collect::<HashMap<_, _>>();
    let sink_wires = [wires.intern("SINK")];
    let spec = SinkRouteSpec {
        criticality: 0.0,
        net_kind: super::RouteNetKind::Generic,
        strict_clock_sink: false,
        ordered_guide: &ordered_guide,
        guide_distances: &guide_distances,
        roots: &[root],
        tree_nodes: &tree_nodes,
        tree_starts: &tree_starts,
        tree_start_costs: &tree_start_costs,
        sink_x: 5,
        sink_y: 5,
        sink_wires: &sink_wires,
    };

    assert_eq!(
        ordered_start_nodes(&spec).into_vec(),
        vec![root, far_tree, near_tree]
    );
}

#[test]
fn dedicated_clock_search_reaches_real_arch_clock_sink() {
    let Some(bundle) =
        ResourceBundle::discover_from(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))).ok()
    else {
        return;
    };
    let arch_path = bundle.root.join("fdp3p7_arch.xml");
    let cil_path = bundle.root.join("fdp3p7_cil.xml");
    if !arch_path.exists() || !cil_path.exists() {
        return;
    }

    let arch = load_arch(&arch_path).expect("load arch");
    let cil = load_cil(&cil_path).expect("load cil");
    let mut wires = WireInterner::default();
    let graphs = load_site_route_graphs(&arch_path, &cil, &mut wires).expect("load graphs");
    let stitch_db = load_tile_stitch_db(&arch_path, &mut wires).expect("load stitch db");
    let stitched_components = build_stitched_components(&stitch_db, &arch, &wires);
    let tile_cache = super::TileRouteCache::build(&arch, &cil, &graphs);
    let root = RouteNode::new(34, 27, wires.intern("CLKB_GCLK1_PW"));
    let sink_wire = wires.intern("S0_CLK_B");
    let ordered_guide = OrderedGuide::new(&[]);
    let guide_distances = GuideDistances::new(&arch, &[]);
    let tree_nodes = std::iter::once(root).collect::<HashSet<_>>();
    let tree_starts = tree_nodes.clone();
    let tree_start_costs = std::iter::once((root, 0usize)).collect::<HashMap<_, _>>();
    let sink_wires = [sink_wire];
    let spec = SinkRouteSpec {
        criticality: 0.0,
        net_kind: super::RouteNetKind::DedicatedClock,
        strict_clock_sink: true,
        ordered_guide: &ordered_guide,
        guide_distances: &guide_distances,
        roots: &[root],
        tree_nodes: &tree_nodes,
        tree_starts: &tree_starts,
        tree_start_costs: &tree_start_costs,
        sink_x: 3,
        sink_y: 31,
        sink_wires: &sink_wires,
    };
    let context = super::RouteSinkContext {
        arch: &arch,
        stitched_components: &stitched_components,
        tile_cache: &tile_cache,
        wires: &mut wires,
    };

    let mut search = super::SearchScratch::default();
    let claims = ClaimIndex::new(1);
    let history = HashMap::default();
    let negotiation = super::occupancy::NegotiationContext {
        claims: &claims,
        history: &history,
        present_factor: 1,
        net_index: 0,
        net_origin: crate::domain::NetOrigin::Logical,
        hard_block: false,
    };
    let path = super::route_sink_with_policy(&context, &negotiation, &mut search, &spec, None);
    let path = path.expect("dedicated clock route");
    let wires_on_path = path
        .nodes
        .iter()
        .map(|node| context.wires.resolve(node.wire).to_string())
        .collect::<Vec<_>>();
    assert!(wires_on_path.iter().any(|wire| wire == "CLKC_VGCLK1"));
    assert!(wires_on_path.iter().any(|wire| wire == "CLKV_GCLK_BUFR1"));
    assert!(!wires_on_path.iter().any(|wire| wire.starts_with("BRAM_")));
}
