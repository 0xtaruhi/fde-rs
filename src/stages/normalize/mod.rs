use crate::{
    ir::Design,
    report::{StageOutput, StageReport},
};
use anyhow::Result;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, Default)]
pub struct NormalizeOptions {
    pub cell_library: Option<PathBuf>,
    pub config: Option<PathBuf>,
}

pub fn run(mut design: Design, options: &NormalizeOptions) -> Result<StageOutput<Design>> {
    let before_cells = design.cells.len();
    design.stage = "normalized".to_string();
    if let Some(cell_library) = &options.cell_library {
        design.note(format!(
            "Normalizer referenced cell library {}",
            cell_library.display()
        ));
    }
    if let Some(config) = &options.config {
        design.note(format!("Normalizer referenced config {}", config.display()));
    }

    design.cells.retain(|cell| {
        !(cell.kind.eq_ignore_ascii_case("buffer")
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
    for (index, cluster) in design.clusters.iter_mut().enumerate() {
        cluster.name = format!("clb_{index:04}");
    }
    let cluster_name_map = design
        .clusters
        .iter()
        .flat_map(|cluster| {
            cluster
                .members
                .iter()
                .cloned()
                .map(move |member| (member, cluster.name.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    for cell in &mut design.cells {
        if let Some(cluster) = cluster_name_map.get(&cell.name) {
            cell.cluster = Some(cluster.clone());
        }
    }
}
