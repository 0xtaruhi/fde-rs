use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct SerializedTextBitstream {
    pub text: String,
    pub notes: Vec<String>,
    pub major_count: usize,
    pub memory_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct TileFrameImage {
    pub tile_name: String,
    pub tile_type: String,
    pub bit_x: usize,
    pub bit_y: usize,
    pub rows: usize,
    pub cols: usize,
    pub bits: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct MajorPayload {
    pub address: usize,
    pub words: Vec<u32>,
}

pub(crate) type TileColumns = BTreeMap<usize, Vec<TileFrameImage>>;

pub(crate) const DEFAULT_FILL_BIT: u8 = 1;
pub(crate) const DEFAULT_MEM_WORDS: usize = 128;
