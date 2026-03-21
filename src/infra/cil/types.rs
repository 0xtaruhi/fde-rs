use std::collections::BTreeMap;

use crate::domain::SiteKind;

#[derive(Debug, Clone, Default)]
pub struct Cil {
    pub device_name: String,
    pub elements: BTreeMap<String, ElementDef>,
    pub sites: BTreeMap<String, SiteDef>,
    pub clusters: BTreeMap<String, ClusterDef>,
    pub transmissions: BTreeMap<String, TransmissionDef>,
    pub tiles: BTreeMap<String, TileDef>,
    pub majors: Vec<MajorFrame>,
    pub bitstream_parameters: BTreeMap<String, String>,
    pub bitstream_commands: Vec<BitstreamCommand>,
}

#[derive(Debug, Clone, Default)]
pub struct ElementDef {
    pub name: String,
    pub default_srams: Vec<SramSetting>,
    pub paths: Vec<ElementPath>,
}

#[derive(Debug, Clone, Default)]
pub struct ElementPath {
    pub input: String,
    pub output: String,
    pub segregated: bool,
    pub configuration: Vec<SramSetting>,
}

#[derive(Debug, Clone, Default)]
pub struct SramSetting {
    pub name: String,
    pub value: u8,
    pub defaulted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SiteDef {
    pub name: String,
    pub config_elements: Vec<SiteConfigElement>,
}

#[derive(Debug, Clone, Default)]
pub struct SiteConfigElement {
    pub name: String,
    pub functions: Vec<SiteFunction>,
}

#[derive(Debug, Clone, Default)]
pub struct SiteFunction {
    pub name: String,
    pub quomodo: String,
    pub manner: String,
    pub is_default: bool,
    pub srams: Vec<SiteFunctionSram>,
}

#[derive(Debug, Clone, Default)]
pub struct SiteFunctionSram {
    pub basic_cell: String,
    pub name: String,
    pub content: u8,
    pub address: Option<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct ClusterDef {
    pub name: String,
    pub site_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct TransmissionDef {
    pub name: String,
    pub site_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct TileDef {
    pub name: String,
    pub sram_rows: usize,
    pub sram_cols: usize,
    pub clusters: Vec<TileCluster>,
    pub transmissions: Vec<TileTransmission>,
}

#[derive(Debug, Clone, Default)]
pub struct TileCluster {
    pub cluster_name: String,
    pub site_type: String,
    pub location: Option<(usize, usize)>,
    pub sites: Vec<TileSite>,
}

#[derive(Debug, Clone, Default)]
pub struct TileTransmission {
    pub transmission_name: String,
    pub site_type: String,
    pub location: Option<(usize, usize)>,
    pub sites: Vec<TileSite>,
}

#[derive(Debug, Clone, Default)]
pub struct TileSite {
    pub name: String,
    pub site_type: String,
    pub position: Option<(usize, usize)>,
    pub srams: Vec<TileSiteSram>,
}

#[derive(Debug, Clone, Default)]
pub struct TileSiteSram {
    pub basic_cell: String,
    pub sram_name: String,
    pub local_place: Option<(usize, usize)>,
    pub owner_tile: Option<String>,
    pub brick_offset: Option<(isize, isize)>,
}

#[derive(Debug, Clone, Default)]
pub struct MajorFrame {
    pub address: usize,
    pub frame_count: usize,
    pub tile_col: usize,
}

#[derive(Debug, Clone, Default)]
pub struct BitstreamCommand {
    pub cmd: String,
    pub parameter: Option<String>,
}

impl Cil {
    pub fn site_def(&self, site_kind: SiteKind) -> Option<&SiteDef> {
        self.sites.get(site_kind.as_str())
    }

    pub fn site_name_for_kind(
        &self,
        tile_type: &str,
        site_kind: SiteKind,
        slot: usize,
    ) -> Option<&str> {
        self.site_name_for_slot(tile_type, site_kind.as_str(), slot)
    }

    pub fn site_name_for_slot(
        &self,
        tile_type: &str,
        site_type: &str,
        slot: usize,
    ) -> Option<&str> {
        let tile = self.tiles.get(tile_type)?;
        let mut candidates = tile
            .clusters
            .iter()
            .flat_map(|cluster| cluster.sites.iter())
            .filter(|site| site.site_type == site_type)
            .collect::<Vec<_>>();
        candidates.sort_by_key(|site| site.position.unwrap_or((0, 0)));
        for preferred in preferred_site_names(site_type, slot) {
            if let Some(site) = candidates.iter().find(|site| site.name == preferred) {
                return Some(site.name.as_str());
            }
        }
        candidates
            .iter()
            .find(|site| site.position.is_some_and(|(_, col)| col == slot))
            .or_else(|| candidates.get(slot))
            .map(|site| site.name.as_str())
    }

    pub fn tile_site(&self, tile_type: &str, site_name: &str) -> Option<&TileSite> {
        self.tiles
            .get(tile_type)?
            .clusters
            .iter()
            .flat_map(|cluster| cluster.sites.iter())
            .find(|site| site.name == site_name)
    }

    pub fn tile_transmission_site(&self, tile_type: &str, site_name: &str) -> Option<&TileSite> {
        self.tiles
            .get(tile_type)?
            .transmissions
            .iter()
            .flat_map(|transmission| transmission.sites.iter())
            .find(|site| site.name == site_name)
    }
}

impl SiteDef {
    pub fn config_element(&self, name: &str) -> Option<&SiteConfigElement> {
        self.config_elements
            .iter()
            .find(|element| element.name == name)
    }
}

impl SiteConfigElement {
    pub fn function(&self, name: &str) -> Option<&SiteFunction> {
        self.functions.iter().find(|function| function.name == name)
    }

    pub fn default_function(&self) -> Option<&SiteFunction> {
        self.functions.iter().find(|function| function.is_default)
    }

    pub fn computation_function(&self, quomodo: &str) -> Option<&SiteFunction> {
        self.functions
            .iter()
            .find(|function| function.quomodo.eq_ignore_ascii_case(quomodo))
    }
}

fn preferred_site_names(site_type: &str, slot: usize) -> [String; 4] {
    [
        format!("{site_type}{slot}"),
        format!("GCLKBUF{slot}"),
        format!("S{slot}"),
        format!("DLL{slot}"),
    ]
}
