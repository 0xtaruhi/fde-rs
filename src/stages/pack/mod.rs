use crate::{
    ir::{CellId, Cluster, Design, DesignIndex},
    report::{StageOutput, StageReport},
};
use anyhow::Result;
use std::{cmp::Ordering, collections::BTreeMap, path::PathBuf};

pub const DEFAULT_PACK_CAPACITY: usize = 4;
const MAX_LUTS_PER_CLUSTER: usize = 2;
const MAX_FFS_PER_CLUSTER: usize = 2;

#[derive(Debug, Clone)]
pub struct PackOptions {
    pub family: Option<String>,
    pub capacity: usize,
    pub cell_library: Option<PathBuf>,
    pub dcp_library: Option<PathBuf>,
    pub config: Option<PathBuf>,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self {
            family: None,
            capacity: DEFAULT_PACK_CAPACITY,
            cell_library: None,
            dcp_library: None,
            config: None,
        }
    }
}

pub fn run(mut design: Design, options: &PackOptions) -> Result<StageOutput<Design>> {
    let capacity = options.capacity.max(2);
    design.stage = "packed".to_string();
    if let Some(family) = &options.family {
        design.metadata.family = family.clone();
    }
    if let Some(cell_library) = &options.cell_library {
        design.note(format!(
            "Pack referenced cell library {}",
            cell_library.display()
        ));
    }
    if let Some(dcp_library) = &options.dcp_library {
        design.note(format!(
            "Pack referenced dc library {}",
            dcp_library.display()
        ));
    }
    if let Some(config) = &options.config {
        design.note(format!("Pack referenced config {}", config.display()));
    }

    let index = design.index();
    let net_drivers = net_driver_cells(&design, &index);
    let connection_graph = build_connection_graph(&design, &index);

    let lanes = build_pack_lanes(&design, &index, &net_drivers, &connection_graph);
    let mut cluster_members = Vec::<Vec<CellId>>::new();
    cluster_members.extend(pair_lane_group(
        &connection_graph,
        &lanes,
        capacity,
        LaneKind::Sequential,
    ));
    cluster_members.extend(pair_lane_group(
        &connection_graph,
        &lanes,
        capacity,
        LaneKind::Lut,
    ));
    cluster_members.extend(pair_lane_group(
        &connection_graph,
        &lanes,
        capacity,
        LaneKind::Other,
    ));

    let clusters = cluster_members
        .iter()
        .enumerate()
        .map(|(cluster_index, members)| {
            Cluster::logic(next_cluster_name(cluster_index))
                .with_members(cell_names(&design, members))
                .with_capacity(capacity)
        })
        .collect::<Vec<_>>();

    for (cluster_index, members) in cluster_members.iter().enumerate() {
        for cell_id in members {
            design.cells[cell_id.index()].cluster = Some(clusters[cluster_index].name.clone());
        }
    }

    design.clusters = clusters;
    let mut report = StageReport::new("pack");
    report.push(format!(
        "Packed {} logical cells into {} clusters (capacity {}).",
        design.cells.len(),
        design.clusters.len(),
        capacity
    ));

    Ok(StageOutput {
        value: design,
        report,
    })
}

fn next_cluster_name(index: usize) -> String {
    format!("clb_{index:04}")
}

fn net_driver_cells(design: &Design, index: &DesignIndex<'_>) -> Vec<Option<CellId>> {
    design
        .nets
        .iter()
        .map(|net| {
            net.driver
                .as_ref()
                .and_then(|driver| index.cell_for_endpoint(driver))
        })
        .collect()
}

type ConnectionGraph = Vec<BTreeMap<CellId, usize>>;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum LaneKind {
    Sequential,
    Lut,
    Other,
}

#[derive(Debug, Clone)]
struct PackLane {
    kind: LaneKind,
    members: Vec<CellId>,
    shape: ClusterShape,
    degree: usize,
    anchor_name: String,
}

impl PackLane {
    fn new(
        design: &Design,
        index: &DesignIndex<'_>,
        graph: &ConnectionGraph,
        kind: LaneKind,
        mut members: Vec<CellId>,
    ) -> Self {
        members.sort();
        let shape = cluster_shape(design, index, &members);
        let degree = members
            .iter()
            .map(|member| graph[member.index()].values().copied().sum::<usize>())
            .sum::<usize>();
        let anchor_name = members
            .iter()
            .map(|member| index.cell(design, *member).name.as_str())
            .min()
            .unwrap_or_default()
            .to_string();
        Self {
            kind,
            members,
            shape,
            degree,
            anchor_name,
        }
    }
}

fn build_connection_graph(design: &Design, index: &DesignIndex<'_>) -> ConnectionGraph {
    let mut graph = vec![BTreeMap::<CellId, usize>::new(); design.cells.len()];
    for net in &design.nets {
        let mut incident = Vec::<CellId>::new();
        if let Some(driver) = &net.driver
            && let Some(driver_id) = index.cell_for_endpoint(driver)
        {
            incident.push(driver_id);
        }
        incident.extend(
            net.sinks
                .iter()
                .filter_map(|sink| index.cell_for_endpoint(sink)),
        );
        incident.sort();
        incident.dedup();
        for (position, lhs) in incident.iter().enumerate() {
            for rhs in incident.iter().skip(position + 1) {
                *graph[lhs.index()].entry(*rhs).or_insert(0) += 1;
                *graph[rhs.index()].entry(*lhs).or_insert(0) += 1;
            }
        }
    }
    graph
}

fn build_pack_lanes(
    design: &Design,
    index: &DesignIndex<'_>,
    net_drivers: &[Option<CellId>],
    graph: &ConnectionGraph,
) -> Vec<PackLane> {
    let mut used = vec![false; design.cells.len()];
    let mut lanes = Vec::new();

    for (cell_index, cell) in design
        .cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.is_sequential())
    {
        let cell_id = CellId::new(cell_index);
        if used[cell_id.index()] {
            continue;
        }
        let mut members = Vec::new();
        let d_net = cell
            .inputs
            .iter()
            .find(|pin| pin.port.eq_ignore_ascii_case("D"))
            .and_then(|pin| index.net_id(&pin.net));
        if let Some(d_net) = d_net
            && let Some(driver_id) = net_drivers[d_net.index()]
            && is_lut_ff_pair(design, index, driver_id, cell_id)
            && !used[driver_id.index()]
        {
            members.push(driver_id);
            used[driver_id.index()] = true;
        }
        members.push(cell_id);
        used[cell_id.index()] = true;
        lanes.push(PackLane::new(
            design,
            index,
            graph,
            LaneKind::Sequential,
            members,
        ));
    }

    for (cell_index, cell) in design.cells.iter().enumerate() {
        let cell_id = CellId::new(cell_index);
        if used[cell_id.index()] || cell.is_constant_source() {
            continue;
        }
        let lane_kind = if cell.is_lut() {
            LaneKind::Lut
        } else {
            LaneKind::Other
        };
        used[cell_id.index()] = true;
        lanes.push(PackLane::new(
            design,
            index,
            graph,
            lane_kind,
            vec![cell_id],
        ));
    }

    lanes
}

fn pair_lane_group(
    graph: &ConnectionGraph,
    lanes: &[PackLane],
    capacity: usize,
    target_kind: LaneKind,
) -> Vec<Vec<CellId>> {
    let mut unused = lanes
        .iter()
        .enumerate()
        .filter_map(|(lane_index, lane)| (lane.kind == target_kind).then_some(lane_index))
        .collect::<Vec<_>>();
    let mut cluster_members = Vec::new();

    if target_kind == LaneKind::Other {
        unused.sort_by(|lhs, rhs| compare_lane_priority(&lanes[*lhs], &lanes[*rhs]));
        cluster_members.extend(
            unused
                .into_iter()
                .map(|lane_id| lanes[lane_id].members.clone()),
        );
        return cluster_members;
    }

    while unused.len() >= 2 {
        let Some((lhs_position, rhs_position)) = best_lane_pair(graph, lanes, &unused, capacity)
        else {
            break;
        };
        let rhs_lane = unused.remove(rhs_position);
        let lhs_lane = unused.remove(lhs_position);
        let mut members = lanes[lhs_lane].members.clone();
        members.extend(lanes[rhs_lane].members.iter().copied());
        members.sort();
        cluster_members.push(members);
    }

    unused.sort_by(|lhs, rhs| compare_lane_priority(&lanes[*lhs], &lanes[*rhs]));
    cluster_members.extend(
        unused
            .into_iter()
            .map(|lane_id| lanes[lane_id].members.clone()),
    );
    cluster_members
}

fn best_lane_pair(
    graph: &ConnectionGraph,
    lanes: &[PackLane],
    unused: &[usize],
    capacity: usize,
) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for lhs_position in 0..unused.len() {
        for rhs_position in (lhs_position + 1)..unused.len() {
            let lhs_lane = &lanes[unused[lhs_position]];
            let rhs_lane = &lanes[unused[rhs_position]];
            if !lhs_lane.shape.can_merge(rhs_lane.shape, capacity) {
                continue;
            }
            let candidate = (lhs_position, rhs_position);
            if best
                .map(|current| {
                    compare_lane_pair(
                        graph,
                        lanes,
                        unused[candidate.0],
                        unused[candidate.1],
                        unused[current.0],
                        unused[current.1],
                    ) == Ordering::Greater
                })
                .unwrap_or(true)
            {
                best = Some(candidate);
            }
        }
    }
    best
}

fn compare_lane_priority(lhs: &PackLane, rhs: &PackLane) -> Ordering {
    lhs.shape
        .total()
        .cmp(&rhs.shape.total())
        .reverse()
        .then_with(|| lhs.degree.cmp(&rhs.degree).reverse())
        .then_with(|| lhs.anchor_name.cmp(&rhs.anchor_name))
}

fn compare_lane_pair(
    graph: &ConnectionGraph,
    lanes: &[PackLane],
    lhs_a: usize,
    lhs_b: usize,
    rhs_a: usize,
    rhs_b: usize,
) -> Ordering {
    let lhs_score = lane_pair_score(graph, lanes, lhs_a, lhs_b);
    let rhs_score = lane_pair_score(graph, lanes, rhs_a, rhs_b);
    lhs_score
        .cmp(&rhs_score)
        .then_with(|| compare_lane_priority(&lanes[lhs_a], &lanes[rhs_a]).reverse())
        .then_with(|| compare_lane_priority(&lanes[lhs_b], &lanes[rhs_b]).reverse())
        .then_with(|| {
            lane_pair_anchor(lanes, lhs_a, lhs_b)
                .cmp(&lane_pair_anchor(lanes, rhs_a, rhs_b))
                .reverse()
        })
}

fn lane_pair_score(
    graph: &ConnectionGraph,
    lanes: &[PackLane],
    lhs_lane: usize,
    rhs_lane: usize,
) -> (usize, usize, usize) {
    let lhs = &lanes[lhs_lane];
    let rhs = &lanes[rhs_lane];
    let shared_weight = lhs
        .members
        .iter()
        .map(|member| {
            rhs.members
                .iter()
                .map(|candidate| {
                    graph[member.index()]
                        .get(candidate)
                        .copied()
                        .unwrap_or_default()
                })
                .sum::<usize>()
        })
        .sum::<usize>();
    (
        shared_weight,
        lhs.shape.total() + rhs.shape.total(),
        lhs.degree + rhs.degree,
    )
}

fn lane_pair_anchor(lanes: &[PackLane], lhs_lane: usize, rhs_lane: usize) -> String {
    let mut names = [
        lanes[lhs_lane].anchor_name.as_str(),
        lanes[rhs_lane].anchor_name.as_str(),
    ];
    names.sort();
    format!("{}|{}", names[0], names[1])
}

fn is_lut_ff_pair(design: &Design, index: &DesignIndex<'_>, lhs: CellId, rhs: CellId) -> bool {
    let (lut_id, ff_id) = match (
        index.cell(design, lhs).is_lut(),
        index.cell(design, lhs).is_sequential(),
        index.cell(design, rhs).is_lut(),
        index.cell(design, rhs).is_sequential(),
    ) {
        (true, false, false, true) => (lhs, rhs),
        (false, true, true, false) => (rhs, lhs),
        _ => return false,
    };
    let lut = index.cell(design, lut_id);
    let ff = index.cell(design, ff_id);
    let Some(d_net) = ff
        .inputs
        .iter()
        .find(|pin| pin.port.eq_ignore_ascii_case("D"))
        .map(|pin| pin.net.as_str())
    else {
        return false;
    };
    lut.outputs.iter().any(|pin| pin.net == d_net)
}

fn cell_names(design: &Design, members: &[CellId]) -> Vec<String> {
    members
        .iter()
        .map(|member| design.cells[member.index()].name.clone())
        .collect()
}

#[derive(Debug, Clone, Copy, Default)]
struct ClusterShape {
    luts: usize,
    ffs: usize,
    others: usize,
}

impl ClusterShape {
    fn include(&mut self, cell: &crate::ir::Cell) {
        if cell.is_lut() {
            self.luts += 1;
        } else if cell.is_sequential() {
            self.ffs += 1;
        } else if !cell.is_constant_source() {
            self.others += 1;
        }
    }

    fn can_merge(self, other: Self, capacity: usize) -> bool {
        let merged = Self {
            luts: self.luts + other.luts,
            ffs: self.ffs + other.ffs,
            others: self.others + other.others,
        };
        merged.total() <= capacity
            && merged.luts <= MAX_LUTS_PER_CLUSTER
            && merged.ffs <= MAX_FFS_PER_CLUSTER
            && merged.others <= 1
    }

    fn total(self) -> usize {
        self.luts + self.ffs + self.others
    }
}

fn cluster_shape(design: &Design, index: &DesignIndex<'_>, members: &[CellId]) -> ClusterShape {
    let mut shape = ClusterShape::default();
    for member in members {
        shape.include(index.cell(design, *member));
    }
    shape
}

#[cfg(test)]
mod tests {
    use super::{PackOptions, run};
    use crate::ir::{Cell, Design, Endpoint, Net};
    use anyhow::Result;

    fn pack_design() -> Design {
        Design {
            name: "pack-mini".to_string(),
            cells: vec![
                Cell::lut("lut_ff_driver", "LUT4").with_output("O", "d_net"),
                Cell::ff("reg0", "DFFHQ")
                    .with_input("D", "d_net")
                    .with_output("Q", "q_net"),
                Cell::lut("lut_a", "LUT4")
                    .with_input("A", "q_net")
                    .with_output("O", "fanout"),
                Cell::lut("lut_b", "LUT4").with_input("A", "fanout"),
            ],
            nets: vec![
                Net::new("d_net")
                    .with_driver(Endpoint::cell("lut_ff_driver", "O"))
                    .with_sink(Endpoint::cell("reg0", "D")),
                Net::new("q_net")
                    .with_driver(Endpoint::cell("reg0", "Q"))
                    .with_sink(Endpoint::cell("lut_a", "A")),
                Net::new("fanout")
                    .with_driver(Endpoint::cell("lut_a", "O"))
                    .with_sink(Endpoint::cell("lut_b", "A")),
            ],
            ..Design::default()
        }
    }

    #[test]
    fn pack_pairs_lut_with_sequential_d_input_and_respects_capacity() -> Result<()> {
        let packed = run(
            pack_design(),
            &PackOptions {
                family: Some("fdp3".to_string()),
                capacity: 2,
                ..PackOptions::default()
            },
        )?
        .value;

        assert_eq!(packed.stage, "packed");
        assert_eq!(packed.metadata.family, "fdp3");
        assert_eq!(packed.clusters.len(), 2);
        assert!(
            packed
                .clusters
                .iter()
                .all(|cluster| cluster.members.len() <= 2)
        );

        let ff_cluster = packed
            .clusters
            .iter()
            .find(|cluster| cluster.members.iter().any(|member| member == "reg0"))
            .expect("cluster containing reg0");
        assert!(
            ff_cluster
                .members
                .iter()
                .any(|member| member == "lut_ff_driver")
        );

        let remaining_cluster = packed
            .clusters
            .iter()
            .find(|cluster| cluster.name != ff_cluster.name)
            .expect("remaining cluster");
        assert_eq!(
            remaining_cluster.members,
            vec!["lut_a".to_string(), "lut_b".to_string()]
        );

        for cell in &packed.cells {
            assert!(
                cell.cluster.is_some(),
                "expected packed cluster for {}",
                cell.name
            );
        }

        Ok(())
    }

    #[test]
    fn pack_scales_across_multiple_independent_lut_ff_pairs() -> Result<()> {
        let mut design = Design {
            name: "pack-many".to_string(),
            ..Design::default()
        };
        for index in 0..8 {
            let lut_name = format!("lut_{index}");
            let ff_name = format!("ff_{index}");
            let net_name = format!("d_net_{index}");
            design
                .cells
                .push(Cell::lut(lut_name.clone(), "LUT4").with_output("O", net_name.clone()));
            design
                .cells
                .push(Cell::ff(ff_name.clone(), "DFFHQ").with_input("D", net_name.clone()));
            design.nets.push(
                Net::new(net_name)
                    .with_driver(Endpoint::cell(lut_name, "O"))
                    .with_sink(Endpoint::cell(ff_name, "D")),
            );
        }

        let packed = run(
            design,
            &PackOptions {
                capacity: 2,
                ..PackOptions::default()
            },
        )?
        .value;

        assert_eq!(packed.clusters.len(), 8);
        for index in 0..8 {
            let lut_name = format!("lut_{index}");
            let ff_name = format!("ff_{index}");
            let cluster = packed
                .clusters
                .iter()
                .find(|cluster| cluster.members.iter().any(|member| member == &ff_name))
                .expect("matching ff cluster");
            assert_eq!(cluster.members.len(), 2);
            assert!(cluster.members.iter().any(|member| member == &lut_name));
        }

        Ok(())
    }

    #[test]
    fn pack_can_fill_four_slot_cluster_along_connected_chain() -> Result<()> {
        let design = Design {
            name: "pack-chain".to_string(),
            cells: vec![
                Cell::lut("lut0", "LUT4").with_output("O", "net0"),
                Cell::ff("ff0", "DFFHQ")
                    .with_input("D", "net0")
                    .with_output("Q", "net1"),
                Cell::lut("lut1", "LUT4")
                    .with_input("A", "net1")
                    .with_output("O", "net2"),
                Cell::ff("ff1", "DFFHQ").with_input("D", "net2"),
            ],
            nets: vec![
                Net::new("net0")
                    .with_driver(Endpoint::cell("lut0", "O"))
                    .with_sink(Endpoint::cell("ff0", "D")),
                Net::new("net1")
                    .with_driver(Endpoint::cell("ff0", "Q"))
                    .with_sink(Endpoint::cell("lut1", "A")),
                Net::new("net2")
                    .with_driver(Endpoint::cell("lut1", "O"))
                    .with_sink(Endpoint::cell("ff1", "D")),
            ],
            ..Design::default()
        };

        let packed = run(
            design,
            &PackOptions {
                family: Some("fdp3".to_string()),
                capacity: 4,
                ..PackOptions::default()
            },
        )?
        .value;

        assert_eq!(packed.clusters.len(), 1);
        assert_eq!(packed.clusters[0].members.len(), 4);
        assert_eq!(
            packed.clusters[0].members,
            vec![
                "lut0".to_string(),
                "ff0".to_string(),
                "lut1".to_string(),
                "ff1".to_string(),
            ]
        );

        Ok(())
    }

    #[test]
    fn pack_respects_slice_shape_limits_when_greedy_fill_expands() -> Result<()> {
        let design = Design {
            name: "pack-shape".to_string(),
            cells: vec![
                Cell::lut("lut0", "LUT4").with_output("O", "net0"),
                Cell::ff("ff0", "DFFHQ")
                    .with_input("D", "net0")
                    .with_output("Q", "net1"),
                Cell::lut("lut1", "LUT4")
                    .with_input("A", "net1")
                    .with_output("O", "net2"),
                Cell::lut("lut2", "LUT4").with_input("A", "net2"),
                Cell::lut("lut3", "LUT4").with_input("A", "net2"),
            ],
            nets: vec![
                Net::new("net0")
                    .with_driver(Endpoint::cell("lut0", "O"))
                    .with_sink(Endpoint::cell("ff0", "D")),
                Net::new("net1")
                    .with_driver(Endpoint::cell("ff0", "Q"))
                    .with_sink(Endpoint::cell("lut1", "A")),
                Net::new("net2")
                    .with_driver(Endpoint::cell("lut1", "O"))
                    .with_sink(Endpoint::cell("lut2", "A"))
                    .with_sink(Endpoint::cell("lut3", "A")),
            ],
            ..Design::default()
        };

        let packed = run(
            design,
            &PackOptions {
                family: Some("fdp3".to_string()),
                capacity: 4,
                ..PackOptions::default()
            },
        )?
        .value;

        assert_eq!(packed.clusters.len(), 3);
        assert!(packed.clusters.iter().all(|cluster| {
            let mut lut_count = 0usize;
            let mut ff_count = 0usize;
            for member in &cluster.members {
                let cell = packed
                    .cells
                    .iter()
                    .find(|cell| cell.name == *member)
                    .expect("cluster member exists");
                if cell.is_lut() {
                    lut_count += 1;
                } else if cell.is_sequential() {
                    ff_count += 1;
                }
            }
            lut_count <= 2 && ff_count <= 2
        }));

        Ok(())
    }

    #[test]
    fn pack_prefers_pairing_ff_lanes_together_before_absorbing_extra_luts() -> Result<()> {
        let design = Design {
            name: "pack-shape-normalization".to_string(),
            cells: vec![
                Cell::lut("lut_ff0", "LUT4").with_output("O", "d0"),
                Cell::ff("ff0", "DFFHQ")
                    .with_input("D", "d0")
                    .with_output("Q", "q0"),
                Cell::lut("lut_ff1", "LUT4").with_output("O", "d1"),
                Cell::ff("ff1", "DFFHQ")
                    .with_input("D", "d1")
                    .with_output("Q", "q1"),
                Cell::lut("lut_ff2", "LUT4").with_output("O", "d2"),
                Cell::ff("ff2", "DFFHQ")
                    .with_input("D", "d2")
                    .with_output("Q", "q2"),
                Cell::lut("lut_a", "LUT4")
                    .with_input("A", "q2")
                    .with_output("O", "a_out"),
                Cell::lut("lut_b", "LUT4").with_input("A", "a_out"),
            ],
            nets: vec![
                Net::new("d0")
                    .with_driver(Endpoint::cell("lut_ff0", "O"))
                    .with_sink(Endpoint::cell("ff0", "D")),
                Net::new("d1")
                    .with_driver(Endpoint::cell("lut_ff1", "O"))
                    .with_sink(Endpoint::cell("ff1", "D")),
                Net::new("d2")
                    .with_driver(Endpoint::cell("lut_ff2", "O"))
                    .with_sink(Endpoint::cell("ff2", "D")),
                Net::new("q2")
                    .with_driver(Endpoint::cell("ff2", "Q"))
                    .with_sink(Endpoint::cell("lut_a", "A")),
                Net::new("a_out")
                    .with_driver(Endpoint::cell("lut_a", "O"))
                    .with_sink(Endpoint::cell("lut_b", "A")),
            ],
            ..Design::default()
        };

        let packed = run(
            design,
            &PackOptions {
                capacity: 4,
                ..PackOptions::default()
            },
        )?
        .value;

        let mut shapes = packed
            .clusters
            .iter()
            .map(|cluster| {
                let mut kinds = cluster
                    .members
                    .iter()
                    .filter_map(|member| packed.cells.iter().find(|cell| &cell.name == member))
                    .map(|cell| cell.kind.as_str().to_ascii_uppercase())
                    .collect::<Vec<_>>();
                kinds.sort();
                kinds
            })
            .collect::<Vec<_>>();
        shapes.sort();

        let mut expected = vec![
            vec![
                "FF".to_string(),
                "FF".to_string(),
                "LUT".to_string(),
                "LUT".to_string(),
            ],
            vec!["FF".to_string(), "LUT".to_string()],
            vec!["LUT".to_string(), "LUT".to_string()],
        ];
        expected.sort();

        assert_eq!(shapes, expected);

        Ok(())
    }
}
