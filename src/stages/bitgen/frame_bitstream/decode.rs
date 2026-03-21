use anyhow::{Context, Result, anyhow, bail};
use std::collections::{BTreeMap, VecDeque};

use crate::{cil::Cil, resource::Arch};

use super::{
    encode::{parameter_u32, parameter_usize, reverse_groups},
    model::{DEFAULT_FILL_BIT, TileColumns, TileFrameImage},
};

pub(crate) fn decode_text_bitstream(arch: &Arch, cil: &Cil, text: &str) -> Result<TileColumns> {
    let mut columns = build_empty_tile_columns(arch, cil);
    let mut blocks_by_address = group_fdri_blocks(parse_fdri_blocks(parse_words(text)?)?);
    let bits_per_group = parameter_usize(cil, "bits_per_grp_reversed")?;
    let major_shift = parameter_u32(cil, "major_shift")?;

    for major in &cil.majors {
        let address = u32::try_from(major.address)
            .map_err(|_| anyhow!("major address {} is too large", major.address))?
            << major_shift;
        let Some(payload) = blocks_by_address
            .get_mut(&address)
            .and_then(VecDeque::pop_front)
        else {
            bail!("missing major payload block for shifted address {address:#x}");
        };

        let Some(column_tiles) = columns.get_mut(&major.tile_col) else {
            bail!(
                "missing tile column {} while decoding major {}",
                major.tile_col,
                major.address
            );
        };
        let frame_bit_count = column_tiles.iter().map(|tile| tile.rows).sum::<usize>();
        let words_per_frame = frame_bit_count.div_ceil(32);
        let expected_words = major.frame_count.saturating_mul(words_per_frame);
        if payload.len() < expected_words {
            bail!(
                "major {} payload has {} words, expected at least {}",
                major.address,
                payload.len(),
                expected_words
            );
        }

        for frame_index in 0..major.frame_count {
            let start = frame_index * words_per_frame;
            let end = start + words_per_frame;
            let mut frame_bits = words_to_bits(&payload[start..end], frame_bit_count);
            reverse_groups(&mut frame_bits, bits_per_group);

            let mut offset = 0usize;
            for tile in column_tiles.iter_mut() {
                if frame_index >= tile.cols {
                    bail!(
                        "tile {} ({}) exposes only {} frames, but decoder saw frame {}",
                        tile.tile_name,
                        tile.tile_type,
                        tile.cols,
                        frame_index
                    );
                }
                for row in 0..tile.rows {
                    tile.bits[row * tile.cols + frame_index] = frame_bits[offset];
                    offset += 1;
                }
            }
        }
    }

    Ok(columns)
}

fn build_empty_tile_columns(arch: &Arch, cil: &Cil) -> TileColumns {
    let mut columns = BTreeMap::<usize, Vec<TileFrameImage>>::new();

    for tile in arch.tiles.values() {
        let Some(tile_def) = cil.tiles.get(&tile.tile_type) else {
            continue;
        };
        columns.entry(tile.bit_y).or_default().push(TileFrameImage {
            tile_name: tile.name.clone(),
            tile_type: tile.tile_type.clone(),
            bit_x: tile.bit_x,
            bit_y: tile.bit_y,
            rows: tile_def.sram_rows,
            cols: tile_def.sram_cols,
            bits: vec![DEFAULT_FILL_BIT; tile_def.sram_rows.saturating_mul(tile_def.sram_cols)],
        });
    }

    for tiles in columns.values_mut() {
        tiles.sort_by(|lhs, rhs| {
            (lhs.bit_x, lhs.tile_name.as_str()).cmp(&(rhs.bit_x, rhs.tile_name.as_str()))
        });
    }

    columns
}

fn parse_words(text: &str) -> Result<Vec<u32>> {
    text.lines()
        .map(|line| line.split_once("//").map_or(line, |(word, _)| word).trim())
        .filter(|line| !line.is_empty())
        .map(|word| {
            u32::from_str_radix(&word.replace('_', ""), 16)
                .with_context(|| format!("failed to parse bitstream word {word}"))
        })
        .collect()
}

fn parse_fdri_blocks(words: Vec<u32>) -> Result<Vec<FdriBlock>> {
    let mut blocks = Vec::new();
    let mut index = 0usize;

    while index + 3 < words.len() {
        if words[index] == 0x3000_2001
            && words[index + 2] == 0x3000_4000
            && (words[index + 3] >> 28) == 0x5
        {
            let address = words[index + 1];
            let count = (words[index + 3] & 0x0fff_ffff) as usize;
            let payload_start = index + 4;
            let payload_end = payload_start + count;
            if payload_end > words.len() {
                bail!(
                    "FDRI block at word {} declares {} payload words, but stream ends early",
                    index,
                    count
                );
            }
            blocks.push(FdriBlock {
                address,
                payload: words[payload_start..payload_end].to_vec(),
            });
            index = payload_end;
            continue;
        }
        index += 1;
    }

    Ok(blocks)
}

fn group_fdri_blocks(blocks: Vec<FdriBlock>) -> BTreeMap<u32, VecDeque<Vec<u32>>> {
    let mut grouped = BTreeMap::<u32, VecDeque<Vec<u32>>>::new();
    for block in blocks {
        grouped
            .entry(block.address)
            .or_default()
            .push_back(block.payload);
    }
    grouped
}

fn words_to_bits(words: &[u32], bit_count: usize) -> Vec<u8> {
    words
        .iter()
        .flat_map(|word| (0..32).rev().map(move |shift| ((word >> shift) & 1) as u8))
        .take(bit_count)
        .collect()
}

struct FdriBlock {
    address: u32,
    payload: Vec<u32>,
}
