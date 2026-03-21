use super::{DeviceCell, DeviceLowering};
use crate::{
    cil::Cil,
    ir::{Cell, Cluster, Design},
    resource::Arch,
};
use std::collections::{BTreeMap, BTreeSet};

impl<'a> DeviceLowering<'a> {
    pub(super) fn materialize_cells(&mut self) {
        let original_cells = lower_original_cells(self.design, self.arch, self.cil);
        let mut lowered_names = self
            .device
            .cells
            .iter()
            .map(|cell| cell.cell_name.clone())
            .collect::<BTreeSet<_>>();
        for cell in original_cells {
            if lowered_names.insert(cell.cell_name.clone()) {
                self.device.cells.push(cell);
            }
        }
    }
}

fn lower_original_cells(design: &Design, arch: &Arch, cil: Option<&Cil>) -> Vec<DeviceCell> {
    let cell_index = design
        .cells
        .iter()
        .map(|cell| (cell.name.clone(), cell))
        .collect::<BTreeMap<_, _>>();
    let mut lowered = Vec::new();
    for cluster in &design.clusters {
        let x = cluster.x.unwrap_or(0);
        let y = cluster.y.unwrap_or(0);
        let tile = arch.tile_at(x, y);
        let tile_name = tile.map(|tile| tile.name.clone()).unwrap_or_default();
        let tile_type = tile
            .map(|tile| tile.tile_type.clone())
            .unwrap_or_else(|| "CENTER".to_string());
        let slice_site_name = cil
            .and_then(|cil| cil.site_name_for_slot(&tile_type, "SLICE", 0))
            .unwrap_or("SLICE")
            .to_string();
        for (cell_name, bel) in assign_cluster_bels(cluster, &cell_index) {
            let Some(cell) = cell_index.get(&cell_name) else {
                continue;
            };
            lowered.push(DeviceCell {
                cell_name: cell.name.clone(),
                type_name: cell.type_name.clone(),
                properties: cell.properties.clone(),
                site_kind: "SLICE".to_string(),
                site_name: slice_site_name.clone(),
                bel,
                tile_name: tile_name.clone(),
                tile_type: tile_type.clone(),
                x,
                y,
                z: 0,
                cluster_name: Some(cluster.name.clone()),
                synthetic: false,
            });
        }
    }

    for cell in design.cells.iter().filter(|cell| cell.cluster.is_none()) {
        let (site_kind, site_name, bel) = if cell.is_constant_source() {
            (
                "CONST".to_string(),
                cell.type_name.clone(),
                "DRV".to_string(),
            )
        } else {
            (
                "UNPLACED".to_string(),
                cell.type_name.clone(),
                "BEL".to_string(),
            )
        };
        lowered.push(DeviceCell {
            cell_name: cell.name.clone(),
            type_name: cell.type_name.clone(),
            properties: cell.properties.clone(),
            site_kind,
            site_name,
            bel,
            tile_name: String::new(),
            tile_type: String::new(),
            x: 0,
            y: 0,
            z: 0,
            cluster_name: None,
            synthetic: false,
        });
    }

    lowered
}

fn assign_cluster_bels(
    cluster: &Cluster,
    cell_index: &BTreeMap<String, &Cell>,
) -> Vec<(String, String)> {
    let mut assigned = Vec::new();
    let mut used = BTreeSet::new();
    let mut slot = 0usize;

    for member in &cluster.members {
        let Some(cell) = cell_index.get(member) else {
            continue;
        };
        if !cell.is_sequential() || used.contains(member) {
            continue;
        }
        if let Some(driver) = lut_feeding_ff(cell, cluster, cell_index)
            && used.insert(driver.clone())
        {
            assigned.push((driver, format!("LUT{slot}")));
        }
        if used.insert(member.clone()) {
            assigned.push((member.clone(), format!("FF{slot}")));
        }
        slot += 1;
    }

    for member in &cluster.members {
        if used.contains(member) {
            continue;
        }
        let Some(cell) = cell_index.get(member) else {
            continue;
        };
        let bel = if cell.is_lut() {
            format!("LUT{slot}")
        } else if cell.is_sequential() {
            format!("FF{slot}")
        } else {
            format!("BEL{slot}")
        };
        assigned.push((member.clone(), bel));
        used.insert(member.clone());
        slot += 1;
    }

    assigned
}

fn lut_feeding_ff(
    ff: &Cell,
    cluster: &Cluster,
    cell_index: &BTreeMap<String, &Cell>,
) -> Option<String> {
    let d_net = ff
        .inputs
        .iter()
        .find(|pin| pin.port.eq_ignore_ascii_case("D"))?
        .net
        .clone();
    cluster.members.iter().find_map(|member| {
        let cell = cell_index.get(member)?;
        if !cell.is_lut() {
            return None;
        }
        cell.outputs
            .iter()
            .any(|pin| pin.net == d_net)
            .then(|| member.clone())
    })
}
