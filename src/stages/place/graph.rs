use crate::ir::Design;
use std::collections::BTreeMap;

pub(crate) type ClusterGraph = BTreeMap<String, BTreeMap<String, f64>>;

pub(crate) fn build_cluster_graph(design: &Design) -> ClusterGraph {
    let mut graph = ClusterGraph::new();
    let cluster_by_cell = design
        .cells
        .iter()
        .filter_map(|cell| {
            cell.cluster
                .as_ref()
                .map(|cluster| (cell.name.clone(), cluster.clone()))
        })
        .collect::<BTreeMap<_, _>>();

    for net in &design.nets {
        let Some(driver) = &net.driver else {
            continue;
        };
        if driver.kind != "cell" {
            continue;
        }
        let Some(src_cluster) = cluster_by_cell.get(&driver.name) else {
            continue;
        };
        let fanout = net.sinks.len().max(1) as f64;
        for sink in &net.sinks {
            if sink.kind != "cell" {
                continue;
            }
            let Some(dst_cluster) = cluster_by_cell.get(&sink.name) else {
                continue;
            };
            if src_cluster == dst_cluster {
                continue;
            }
            *graph
                .entry(src_cluster.clone())
                .or_default()
                .entry(dst_cluster.clone())
                .or_insert(0.0) += 1.0 / fanout;
            *graph
                .entry(dst_cluster.clone())
                .or_default()
                .entry(src_cluster.clone())
                .or_insert(0.0) += 1.0 / fanout;
        }
    }

    graph
}

pub(crate) fn weighted_centroid(
    cluster_name: &str,
    graph: &ClusterGraph,
    placements: &BTreeMap<String, (usize, usize)>,
) -> Option<(usize, usize)> {
    let mut x_total = 0.0;
    let mut y_total = 0.0;
    let mut weight_total = 0.0;
    for (neighbor, weight) in graph.get(cluster_name)? {
        let Some((x, y)) = placements.get(neighbor) else {
            continue;
        };
        x_total += *x as f64 * weight;
        y_total += *y as f64 * weight;
        weight_total += weight;
    }
    if weight_total == 0.0 {
        None
    } else {
        Some((
            (x_total / weight_total).round() as usize,
            (y_total / weight_total).round() as usize,
        ))
    }
}

pub(crate) fn cluster_incident_criticality(design: &Design) -> BTreeMap<String, f64> {
    let cluster_by_cell = design
        .cells
        .iter()
        .filter_map(|cell| {
            cell.cluster
                .as_ref()
                .map(|cluster| (cell.name.clone(), cluster.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let mut totals = BTreeMap::<String, f64>::new();
    for net in &design.nets {
        let weight = 1.0 + net.criticality.max(0.0);
        if let Some(driver) = &net.driver
            && let Some(cluster) = cluster_by_cell.get(&driver.name)
        {
            *totals.entry(cluster.clone()).or_insert(0.0) += weight;
        }
        for sink in &net.sinks {
            if let Some(cluster) = cluster_by_cell.get(&sink.name) {
                *totals.entry(cluster.clone()).or_insert(0.0) += weight;
            }
        }
    }
    totals
}
