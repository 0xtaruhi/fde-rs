use crate::{
    cil::{TileSite, TileSiteSram},
    config_image::types::ResolvedSiteBit,
    device::DeviceCell,
    route_bits::RouteBit,
};

pub(crate) fn find_site_sram<'a>(
    tile_site: &'a TileSite,
    basic_cell: &str,
    sram_name: &str,
) -> Option<&'a TileSiteSram> {
    tile_site
        .srams
        .iter()
        .find(|sram| sram.basic_cell == basic_cell && sram.sram_name == sram_name)
        .or_else(|| {
            tile_site.srams.iter().find(|sram| {
                sram.basic_cell.is_empty() && basic_cell.is_empty() && sram.sram_name == sram_name
            })
        })
}

pub(crate) fn find_tile_sram<'a>(
    tile_site: &'a TileSite,
    bit: &ResolvedSiteBit,
) -> Option<&'a TileSiteSram> {
    find_site_sram(tile_site, &bit.basic_cell, &bit.sram_name)
}

pub(crate) fn find_route_sram<'a>(
    tile_site: &'a TileSite,
    bit: &RouteBit,
) -> Option<&'a TileSiteSram> {
    tile_site
        .srams
        .iter()
        .find(|sram| sram.basic_cell == bit.basic_cell && sram.sram_name == bit.sram_name)
}

pub(crate) fn cell_property<'a>(cell: &'a DeviceCell, key: &str) -> Option<&'a str> {
    cell.properties
        .iter()
        .find(|property| property.key.eq_ignore_ascii_case(key))
        .map(|property| property.value.as_str())
}

pub(crate) fn bel_slot(bel: &str) -> Option<usize> {
    bel.chars()
        .rev()
        .find(|ch| ch.is_ascii_digit())
        .and_then(|ch| ch.to_digit(10))
        .map(|digit| digit as usize)
}

pub(crate) fn is_lut_type(type_name: &str) -> bool {
    type_name.to_ascii_lowercase().starts_with("lut")
}

pub(crate) fn is_ff_type(type_name: &str) -> bool {
    let type_name = type_name.to_ascii_lowercase();
    type_name.contains("ff") || type_name.contains("latch")
}
