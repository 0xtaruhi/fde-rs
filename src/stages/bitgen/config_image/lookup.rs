use crate::{
    cil::{TileSite, TileSiteSram},
    route::RouteBit,
};

use super::types::ResolvedSiteBit;

pub(crate) fn find_tile_sram<'a>(
    tile_site: &'a TileSite,
    bit: &ResolvedSiteBit,
) -> Option<&'a TileSiteSram> {
    tile_site
        .srams
        .iter()
        .find(|sram| sram.basic_cell == bit.basic_cell && sram.sram_name == bit.sram_name)
        .or_else(|| {
            tile_site.srams.iter().find(|sram| {
                sram.basic_cell.is_empty()
                    && bit.basic_cell.is_empty()
                    && sram.sram_name == bit.sram_name
            })
        })
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
