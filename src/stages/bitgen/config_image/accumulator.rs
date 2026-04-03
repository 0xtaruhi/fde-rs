use super::types::{AppliedSiteConfig, TileBitAssignment, TileConfigImage};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct TileAccumulator {
    tile_name: String,
    tile_type: String,
    x: usize,
    y: usize,
    rows: usize,
    cols: usize,
    configs: BTreeSet<(String, String, String)>,
    assignments: BTreeMap<(usize, usize), TileBitAssignment>,
}

impl TileAccumulator {
    pub(crate) fn new_tile(
        tile_name: &str,
        tile_type: &str,
        x: usize,
        y: usize,
        rows: usize,
        cols: usize,
    ) -> Self {
        Self {
            tile_name: tile_name.to_string(),
            tile_type: tile_type.to_string(),
            x,
            y,
            rows,
            cols,
            configs: BTreeSet::new(),
            assignments: BTreeMap::new(),
        }
    }

    pub(crate) fn configs_mut(&mut self) -> &mut BTreeSet<(String, String, String)> {
        &mut self.configs
    }

    pub(crate) fn insert(&mut self, assignment: TileBitAssignment) {
        self.assignments
            .entry((assignment.row, assignment.col))
            .and_modify(|existing| {
                assert_eq!(
                    existing.value,
                    assignment.value,
                    "conflicting bit assignment on tile {} ({}, {}) at B{}W{}: \
existing {}:{}:{} {}:{}={} vs new {}:{}:{} {}:{}={}",
                    self.tile_name,
                    self.x,
                    self.y,
                    assignment.row,
                    assignment.col,
                    existing.site_name,
                    existing.cfg_name,
                    existing.function_name,
                    existing.basic_cell,
                    existing.sram_name,
                    existing.value,
                    assignment.site_name,
                    assignment.cfg_name,
                    assignment.function_name,
                    assignment.basic_cell,
                    assignment.sram_name,
                    assignment.value
                );
                *existing = assignment.clone();
            })
            .or_insert(assignment);
    }

    pub(crate) fn finish(self) -> TileConfigImage {
        TileConfigImage {
            tile_name: self.tile_name,
            tile_type: self.tile_type,
            x: self.x,
            y: self.y,
            rows: self.rows,
            cols: self.cols,
            configs: self
                .configs
                .into_iter()
                .map(|(site_name, cfg_name, function_name)| AppliedSiteConfig {
                    site_name,
                    cfg_name,
                    function_name,
                })
                .collect(),
            assignments: self.assignments.into_values().collect(),
        }
    }
}
