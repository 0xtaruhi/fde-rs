use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::{
    Cell, Cluster, DesignIndex, Net, Placement, PlacementSite, Port, RoutePip, SliceBinding,
    SliceBindingKind, TimingSummary,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Design {
    pub name: String,
    pub stage: String,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub ports: Vec<Port>,
    #[serde(default)]
    pub cells: Vec<Cell>,
    #[serde(default)]
    pub nets: Vec<Net>,
    #[serde(default)]
    pub clusters: Vec<Cluster>,
    #[serde(default)]
    pub timing: Option<TimingSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metadata {
    #[serde(default)]
    pub source_format: String,
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub arch_name: String,
    #[serde(default)]
    pub lut_size: usize,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl Design {
    pub fn index(&self) -> DesignIndex<'_> {
        DesignIndex::build(self)
    }

    pub fn note(&mut self, note: impl Into<String>) {
        self.metadata.notes.push(note.into());
    }

    pub fn note_once(&mut self, note: impl Into<String>) {
        let note = note.into();
        if !self.metadata.notes.iter().any(|existing| existing == &note) {
            self.metadata.notes.push(note);
        }
    }

    pub fn used_pins(&self) -> BTreeSet<String> {
        self.ports
            .iter()
            .filter_map(|port| port.pin.clone())
            .collect::<BTreeSet<_>>()
    }

    pub fn placement(&self) -> Placement {
        Placement {
            sites: self
                .clusters
                .iter()
                .filter_map(|cluster| {
                    Some(PlacementSite {
                        cluster: cluster.name.clone(),
                        x: cluster.x?,
                        y: cluster.y?,
                        z: cluster.z.unwrap_or(0),
                        fixed: cluster.fixed,
                    })
                })
                .collect(),
        }
    }

    pub fn infer_slice_bindings_from_route_pips(&mut self) {
        for cell in &mut self.cells {
            cell.slice_binding = None;
        }

        let updates = self
            .nets
            .iter()
            .filter_map(|net| {
                let binding = physical_net_binding(&net.route_pips)?;
                let driver_name = net
                    .driver
                    .as_ref()
                    .filter(|endpoint| endpoint.is_cell())
                    .map(|endpoint| endpoint.name.clone())?;
                Some((driver_name, binding))
            })
            .collect::<Vec<_>>();

        for (driver_name, binding) in updates {
            if let Some(cell) = self.cells.iter_mut().find(|cell| cell.name == driver_name) {
                cell.slice_binding = Some(binding);
            }
        }

        propagate_slice_pair_bindings(self);
    }
}

fn physical_net_binding(route_pips: &[RoutePip]) -> Option<SliceBinding> {
    route_pips
        .iter()
        .find_map(|pip| parse_slice_output_wire(&pip.from_net))
}

fn parse_slice_output_wire(wire: &str) -> Option<SliceBinding> {
    let (_site, suffix) = wire.split_once('_')?;
    match suffix {
        "X" => Some(SliceBinding {
            slot: 0,
            kind: SliceBindingKind::Lut,
        }),
        "Y" => Some(SliceBinding {
            slot: 1,
            kind: SliceBindingKind::Lut,
        }),
        "XQ" => Some(SliceBinding {
            slot: 0,
            kind: SliceBindingKind::Sequential,
        }),
        "YQ" => Some(SliceBinding {
            slot: 1,
            kind: SliceBindingKind::Sequential,
        }),
        _ => None,
    }
}

fn propagate_slice_pair_bindings(design: &mut Design) {
    let mut changed = true;
    while changed {
        changed = false;
        let index = design.index();
        let mut pending = Vec::new();
        for cluster_id in 0..design.clusters.len() {
            let cluster_id = crate::ir::ClusterId::new(cluster_id);
            for &ff_id in index.cluster_members(cluster_id) {
                let ff = index.cell(design, ff_id);
                if !ff.is_sequential() {
                    continue;
                }
                let Some(ff_binding) = ff.slice_binding else {
                    continue;
                };
                let Some(d_net) = ff
                    .inputs
                    .iter()
                    .find(|pin| pin.port.eq_ignore_ascii_case("D"))
                    .map(|pin| pin.net.as_str())
                else {
                    continue;
                };
                let Some(driver_id) =
                    index
                        .cluster_members(cluster_id)
                        .iter()
                        .find_map(|cell_id| {
                            let cell = index.cell(design, *cell_id);
                            if !cell.is_lut() {
                                return None;
                            }
                            cell.outputs
                                .iter()
                                .any(|pin| pin.net == d_net)
                                .then_some(*cell_id)
                        })
                else {
                    continue;
                };
                let Some(driver) = design.cells.get(driver_id.index()) else {
                    continue;
                };
                if driver.slice_binding.is_none() {
                    pending.push((driver_id.index(), ff_binding.slot));
                }
            }
        }
        for (cell_index, slot) in pending {
            if let Some(cell) = design.cells.get_mut(cell_index)
                && cell.slice_binding.is_none()
            {
                cell.slice_binding = Some(SliceBinding {
                    slot,
                    kind: SliceBindingKind::Lut,
                });
                changed = true;
            }
        }
    }
}
