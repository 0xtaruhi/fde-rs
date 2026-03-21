use super::{DeviceCell, DeviceLowering};
use crate::{
    cil::Cil,
    domain::SiteKind,
    ir::{CellId, ClusterId, Design, DesignIndex},
    resource::Arch,
};
use std::collections::BTreeSet;

impl<'a> DeviceLowering<'a> {
    pub(super) fn materialize_cells(&mut self) {
        let lowered = lower_original_cells(self.design, &self.index, self.arch, self.cil);
        let mut seen_names = self
            .device
            .cells
            .iter()
            .map(|cell| cell.cell_name.clone())
            .collect::<BTreeSet<_>>();
        for (cell_id, cell) in lowered {
            if !seen_names.insert(cell.cell_name.clone()) {
                continue;
            }
            self.push_original_cell(cell_id, cell);
        }
    }
}

fn lower_original_cells(
    design: &Design,
    index: &DesignIndex<'_>,
    arch: &Arch,
    cil: Option<&Cil>,
) -> Vec<(CellId, DeviceCell)> {
    let mut lowered = Vec::new();
    for cluster_index in 0..design.clusters.len() {
        let cluster_id = ClusterId::new(cluster_index);
        let cluster = index.cluster(design, cluster_id);
        let x = cluster.x.unwrap_or(0);
        let y = cluster.y.unwrap_or(0);
        let z = cluster.z.unwrap_or(0);
        let tile = arch.tile_at(x, y);
        let tile_name = tile.map(|tile| tile.name.clone()).unwrap_or_default();
        let tile_type = tile
            .map(|tile| tile.tile_type.clone())
            .unwrap_or_else(|| "CENTER".to_string());
        let slice_site_name = cil
            .and_then(|cil| cil.site_name_for_kind(&tile_type, SiteKind::LogicSlice, z))
            .unwrap_or("SLICE")
            .to_string();
        for (cell_id, bel) in assign_cluster_bels(design, index, cluster_id) {
            let cell = index.cell(design, cell_id);
            lowered.push((
                cell_id,
                DeviceCell::new(cell.name.clone(), cell.type_name.clone())
                    .with_properties(cell.properties.clone())
                    .placed(
                        SiteKind::LogicSlice,
                        slice_site_name.clone(),
                        bel,
                        tile_name.clone(),
                        tile_type.clone(),
                        (x, y, z),
                    )
                    .in_cluster(cluster.name.clone()),
            ));
        }
    }

    for (cell_index, cell) in design
        .cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.cluster.is_none())
    {
        let (site_kind, site_name, bel) = if cell.is_constant_source() {
            (SiteKind::Const, cell.type_name.clone(), "DRV".to_string())
        } else {
            (
                SiteKind::Unplaced,
                cell.type_name.clone(),
                "BEL".to_string(),
            )
        };
        lowered.push((
            CellId::new(cell_index),
            DeviceCell::new(cell.name.clone(), cell.type_name.clone())
                .with_properties(cell.properties.clone())
                .placed(
                    site_kind,
                    site_name,
                    bel,
                    String::new(),
                    String::new(),
                    (0, 0, 0),
                ),
        ));
    }

    lowered
}

fn assign_cluster_bels(
    design: &Design,
    index: &DesignIndex<'_>,
    cluster_id: ClusterId,
) -> Vec<(CellId, String)> {
    let mut assigned = Vec::new();
    let mut used = BTreeSet::<CellId>::new();
    let mut slot = 0usize;

    for &cell_id in index.cluster_members(cluster_id) {
        let cell = index.cell(design, cell_id);
        if !cell.is_sequential() || used.contains(&cell_id) {
            continue;
        }
        if let Some(driver_id) = lut_feeding_ff(design, index, cell_id, cluster_id)
            && used.insert(driver_id)
        {
            assigned.push((driver_id, format!("LUT{slot}")));
        }
        if used.insert(cell_id) {
            assigned.push((cell_id, format!("FF{slot}")));
        }
        slot += 1;
    }

    for &cell_id in index.cluster_members(cluster_id) {
        if used.contains(&cell_id) {
            continue;
        }
        let cell = index.cell(design, cell_id);
        let bel = if cell.is_lut() {
            format!("LUT{slot}")
        } else if cell.is_sequential() {
            format!("FF{slot}")
        } else {
            format!("BEL{slot}")
        };
        assigned.push((cell_id, bel));
        used.insert(cell_id);
        slot += 1;
    }

    assigned
}

fn lut_feeding_ff(
    design: &Design,
    index: &DesignIndex<'_>,
    ff_id: CellId,
    cluster_id: ClusterId,
) -> Option<CellId> {
    let ff = index.cell(design, ff_id);
    let d_net = ff
        .inputs
        .iter()
        .find(|pin| pin.port.eq_ignore_ascii_case("D"))?
        .net
        .as_str();
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
}
