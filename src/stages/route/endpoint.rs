use crate::{
    DeviceCell, DeviceDesign, DeviceDesignIndex, DeviceEndpoint, DeviceEndpointRef, DevicePort,
};

pub(super) fn resolve_route_endpoint<'a>(
    device: &'a DeviceDesign,
    index: &DeviceDesignIndex<'a>,
    endpoint: &DeviceEndpoint,
) -> ResolvedRouteEndpoint<'a> {
    index.resolve_endpoint_ref(device, endpoint).into()
}

pub(super) enum ResolvedRouteEndpoint<'a> {
    Cell(&'a DeviceCell),
    Port(&'a DevicePort),
    Unknown,
}

impl<'a> From<DeviceEndpointRef<'a>> for ResolvedRouteEndpoint<'a> {
    fn from(value: DeviceEndpointRef<'a>) -> Self {
        match value {
            DeviceEndpointRef::Cell(cell) => Self::Cell(cell),
            DeviceEndpointRef::Port(port) => Self::Port(port),
            DeviceEndpointRef::Unknown => Self::Unknown,
        }
    }
}
