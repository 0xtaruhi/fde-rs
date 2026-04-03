use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigImage {
    #[serde(default)]
    pub tiles: Vec<TileConfigImage>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TileConfigImage {
    pub tile_name: String,
    pub tile_type: String,
    pub x: usize,
    pub y: usize,
    pub rows: usize,
    pub cols: usize,
    #[serde(default)]
    pub configs: Vec<AppliedSiteConfig>,
    #[serde(default)]
    pub assignments: Vec<TileBitAssignment>,
}

impl TileConfigImage {
    pub fn set_bit_count(&self) -> usize {
        self.assignments.iter().filter(|bit| bit.value != 0).count()
    }

    pub fn packed_bits(&self) -> Vec<u8> {
        let mut grid = vec![0u8; self.rows.saturating_mul(self.cols)];
        for bit in &self.assignments {
            let index = bit.row.saturating_mul(self.cols).saturating_add(bit.col);
            if index < grid.len() {
                grid[index] = bit.value;
            }
        }
        let mut packed = Vec::with_capacity(grid.len().div_ceil(8));
        for chunk in grid.chunks(8) {
            let mut byte = 0u8;
            for (index, bit) in chunk.iter().enumerate() {
                byte |= (*bit & 1) << index;
            }
            packed.push(byte);
        }
        packed
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppliedSiteConfig {
    pub site_name: String,
    pub cfg_name: String,
    pub function_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TileBitAssignment {
    pub site_name: String,
    pub cfg_name: String,
    pub function_name: String,
    pub basic_cell: String,
    pub sram_name: String,
    pub row: usize,
    pub col: usize,
    pub value: u8,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedSiteBit {
    pub(crate) cfg_name: String,
    pub(crate) function_name: String,
    pub(crate) basic_cell: String,
    pub(crate) sram_name: String,
    pub(crate) value: u8,
}

pub(crate) enum ConfigResolution {
    Matched(Vec<ResolvedSiteBit>),
    Unmatched,
}
