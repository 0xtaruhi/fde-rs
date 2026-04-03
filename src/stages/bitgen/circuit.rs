use crate::ir::{Cluster, Design, Net};

#[derive(Debug, Clone)]
pub(super) struct BitgenCircuit {
    pub(super) design_name: String,
    pub(super) stage_name: String,
    pub(super) clusters: Vec<Cluster>,
    pub(super) nets: Vec<Net>,
}

impl BitgenCircuit {
    pub(super) fn from_design(design: &Design) -> Self {
        Self {
            design_name: design.name.clone(),
            stage_name: design.stage.clone(),
            clusters: sorted_clusters(design),
            nets: sorted_nets(design),
        }
    }
}

fn sorted_clusters(design: &Design) -> Vec<Cluster> {
    let mut clusters = design.clusters.clone();
    clusters.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    clusters
}

fn sorted_nets(design: &Design) -> Vec<Net> {
    let mut nets = design.nets.clone();
    nets.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    nets
}
