use crate::{
    ir::{CellKind, Design},
    report::{StageOutput, StageReport},
};
use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct NormalizeOptions {
    pub cell_library: Option<PathBuf>,
    pub config: Option<PathBuf>,
}

pub fn run(mut design: Design, _options: &NormalizeOptions) -> Result<StageOutput<Design>> {
    let before_cells = design.cells.len();
    design.stage = "normalized".to_string();

    design.cells.retain(|cell| {
        !(cell.kind == CellKind::Buffer
            && cell.inputs.len() == 1
            && cell.outputs.len() == 1
            && cell.inputs[0].net == cell.outputs[0].net)
    });
    prune_disconnected_nets(&mut design);
    canonicalize_names(&mut design);

    let mut report = StageReport::new("normalize");
    report.push(format!(
        "Normalized design: {} -> {} cells, {} nets remain.",
        before_cells,
        design.cells.len(),
        design.nets.len()
    ));

    Ok(StageOutput {
        value: design,
        report,
    })
}

pub(crate) fn prune_disconnected_nets(design: &mut Design) {
    design
        .nets
        .retain(|net| net.driver.is_some() || !net.sinks.is_empty());
}

pub(crate) fn canonicalize_names(design: &mut Design) {
    if design.clusters.is_empty() {
        return;
    }
    let cluster_members = {
        let index = design.index();
        (0..design.clusters.len())
            .map(|cluster_index| {
                let cluster_id = crate::ir::ClusterId::new(cluster_index);
                index.cluster_members(cluster_id).to_vec()
            })
            .collect::<Vec<_>>()
    };
    for (index, cluster) in design.clusters.iter_mut().enumerate() {
        cluster.name = format!("clb_{index:04}");
    }
    for (cluster_index, members) in cluster_members.into_iter().enumerate() {
        let cluster_name = design.clusters[cluster_index].name.clone();
        for cell_id in members {
            if let Some(cell) = design.cells.get_mut(cell_id.index()) {
                cell.cluster = Some(cluster_name.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::canonicalize_names;
    use crate::ir::{Cell, Cluster, Design};

    #[test]
    fn canonicalize_names_renames_clusters_and_syncs_cell_membership() {
        let mut design = Design {
            cells: vec![
                Cell::lut("u0", "LUT4").in_cluster("old_a"),
                Cell::lut("u1", "LUT4").in_cluster("old_b"),
                Cell::lut("u2", "LUT4"),
            ],
            clusters: vec![
                Cluster::logic("old_a").with_member("u0").with_capacity(1),
                Cluster::logic("old_b").with_member("u1").with_capacity(1),
            ],
            ..Design::default()
        };

        canonicalize_names(&mut design);

        assert_eq!(design.clusters[0].name, "clb_0000");
        assert_eq!(design.clusters[1].name, "clb_0001");
        assert_eq!(design.cells[0].cluster.as_deref(), Some("clb_0000"));
        assert_eq!(design.cells[1].cluster.as_deref(), Some("clb_0001"));
        assert!(design.cells[2].cluster.is_none());
    }
}
