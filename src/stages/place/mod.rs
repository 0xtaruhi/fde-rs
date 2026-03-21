mod cost;
mod graph;
mod model;
mod solver;

use crate::{
    analysis::annotate_net_criticality,
    constraints::{ConstraintEntry, apply_constraints, ensure_port_positions},
    ir::Design,
    report::{StageOutput, StageReport},
    resource::{Arch, DelayModel},
};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlaceMode {
    BoundingBox,
    TimingDriven,
}

#[derive(Debug, Clone)]
pub struct PlaceOptions {
    pub arch: Arch,
    pub delay: Option<DelayModel>,
    pub constraints: Vec<ConstraintEntry>,
    pub mode: PlaceMode,
    pub seed: u64,
}

pub fn run(mut design: Design, options: &PlaceOptions) -> Result<StageOutput<Design>> {
    design.stage = "placed".to_string();
    design.metadata.arch_name = options.arch.name.clone();
    apply_constraints(&mut design, &options.arch, &options.constraints);
    ensure_port_positions(&mut design, &options.arch);

    if matches!(options.mode, PlaceMode::TimingDriven) {
        annotate_net_criticality(&mut design);
    }

    if design.clusters.is_empty() {
        let mut report = StageReport::new("place");
        report.push("Design contains no clusters; placement only updated IO anchors.".to_string());
        return Ok(StageOutput {
            value: design,
            report,
        });
    }

    let sites = options.arch.logic_sites();
    if design.clusters.len() > sites.len() {
        bail!(
            "not enough logic sites: need {}, only {} available",
            design.clusters.len(),
            sites.len()
        );
    }

    let solution = solver::solve(&design, options)?;
    for cluster in &mut design.clusters {
        if let Some((x, y)) = solution.placements.get(&cluster.name) {
            cluster.x = Some(*x);
            cluster.y = Some(*y);
        }
    }

    let mut report = StageReport::new("place");
    report.push(format!(
        "Placed {} clusters on a {}x{} grid with final cost {:.3}.",
        design.clusters.len(),
        options.arch.width,
        options.arch.height,
        solution.metrics.total
    ));
    report.push(format!(
        "Placement components: wire {:.3}, congestion {:.3}, timing {:.3}, locality {:.3}.",
        solution.metrics.wire_cost,
        solution.metrics.congestion_cost,
        solution.metrics.timing_cost,
        solution.metrics.locality_cost
    ));

    Ok(StageOutput {
        value: design,
        report,
    })
}

pub(crate) fn manhattan(lhs: (usize, usize), rhs: (usize, usize)) -> usize {
    lhs.0.abs_diff(rhs.0) + lhs.1.abs_diff(rhs.1)
}
