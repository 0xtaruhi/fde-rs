use crate::{
    domain::{EndpointKind, NetOrigin, PrimitiveKind, SiteKind},
    ir::{PortDirection, Property, RoutePip},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceDesign {
    pub name: String,
    pub device: String,
    pub ports: Vec<DevicePort>,
    pub cells: Vec<DeviceCell>,
    pub nets: Vec<DeviceNet>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DevicePort {
    pub port_name: String,
    pub direction: PortDirection,
    pub pin_name: String,
    pub site_kind: String,
    pub site_name: String,
    pub tile_name: String,
    pub tile_type: String,
    pub x: usize,
    pub y: usize,
    pub z: usize,
}

impl DevicePort {
    pub fn site_kind_class(&self) -> SiteKind {
        SiteKind::classify(&self.site_kind)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceCell {
    pub cell_name: String,
    pub type_name: String,
    #[serde(default)]
    pub properties: Vec<Property>,
    pub site_kind: String,
    pub site_name: String,
    pub bel: String,
    pub tile_name: String,
    pub tile_type: String,
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub cluster_name: Option<String>,
    pub synthetic: bool,
}

impl DeviceCell {
    pub fn primitive_kind(&self) -> PrimitiveKind {
        PrimitiveKind::classify(&self.type_name, &self.type_name)
    }

    pub fn site_kind_class(&self) -> SiteKind {
        SiteKind::classify(&self.site_kind)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceNet {
    pub name: String,
    pub driver: Option<DeviceEndpoint>,
    pub sinks: Vec<DeviceEndpoint>,
    pub origin: String,
    #[serde(default)]
    pub route_pips: Vec<RoutePip>,
    #[serde(default)]
    pub guide_tiles: Vec<(usize, usize)>,
    #[serde(default)]
    pub sink_guides: Vec<DeviceSinkGuide>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceSinkGuide {
    pub sink: DeviceEndpoint,
    #[serde(default)]
    pub tiles: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceEndpoint {
    pub kind: String,
    pub name: String,
    pub pin: String,
    pub x: usize,
    pub y: usize,
    pub z: usize,
}

impl DeviceNet {
    pub fn origin_kind(&self) -> NetOrigin {
        NetOrigin::classify(&self.origin)
    }

    pub fn guide_tiles_for_sink<'a>(&'a self, sink: &DeviceEndpoint) -> &'a [(usize, usize)] {
        self.sink_guides
            .iter()
            .find(|guide| guide.sink == *sink && !guide.tiles.is_empty())
            .map(|guide| guide.tiles.as_slice())
            .unwrap_or(self.guide_tiles.as_slice())
    }
}

impl DeviceEndpoint {
    pub fn endpoint_kind(&self) -> EndpointKind {
        EndpointKind::classify(&self.kind)
    }

    pub fn is_cell(&self) -> bool {
        self.endpoint_kind().is_cell()
    }

    pub fn is_port(&self) -> bool {
        self.endpoint_kind().is_port()
    }
}
