use std::collections::HashMap;

use crate::domain::EndpointKind;

use super::types::{DeviceCell, DeviceDesign, DeviceEndpoint, DevicePort};

macro_rules! define_device_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub(crate) struct $name(usize);

        impl $name {
            pub(crate) const fn new(index: usize) -> Self {
                Self(index)
            }
        }
    };
}

define_device_id!(DevicePortId);
define_device_id!(DeviceCellId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum DeviceEndpointTarget {
    Cell(DeviceCellId),
    Port(DevicePortId),
    Unknown,
}

pub(crate) enum DeviceEndpointRef<'a> {
    Cell(&'a DeviceCell),
    Port(&'a DevicePort),
    Unknown,
}

#[derive(Debug, Clone)]
pub(crate) struct DeviceDesignIndex<'a> {
    ports_by_name: HashMap<&'a str, DevicePortId>,
    cells_by_name: HashMap<&'a str, DeviceCellId>,
}

impl<'a> DeviceDesignIndex<'a> {
    pub(crate) fn build(device: &'a DeviceDesign) -> Self {
        let ports_by_name = device
            .ports
            .iter()
            .enumerate()
            .map(|(index, port)| (port.port_name.as_str(), DevicePortId::new(index)))
            .collect::<HashMap<_, _>>();
        let cells_by_name = device
            .cells
            .iter()
            .enumerate()
            .map(|(index, cell)| (cell.cell_name.as_str(), DeviceCellId::new(index)))
            .collect::<HashMap<_, _>>();
        Self {
            ports_by_name,
            cells_by_name,
        }
    }

    pub(crate) fn port_id(&self, name: &str) -> Option<DevicePortId> {
        self.ports_by_name.get(name).copied()
    }

    pub(crate) fn cell_id(&self, name: &str) -> Option<DeviceCellId> {
        self.cells_by_name.get(name).copied()
    }

    pub(crate) fn resolve_endpoint(&self, endpoint: &DeviceEndpoint) -> DeviceEndpointTarget {
        match endpoint.kind {
            EndpointKind::Cell => self
                .cell_id(&endpoint.name)
                .map(DeviceEndpointTarget::Cell)
                .unwrap_or(DeviceEndpointTarget::Unknown),
            EndpointKind::Port => self
                .port_id(&endpoint.name)
                .map(DeviceEndpointTarget::Port)
                .unwrap_or(DeviceEndpointTarget::Unknown),
            EndpointKind::Unknown => DeviceEndpointTarget::Unknown,
        }
    }

    pub(crate) fn resolve_endpoint_ref(
        &self,
        device: &'a DeviceDesign,
        endpoint: &DeviceEndpoint,
    ) -> DeviceEndpointRef<'a> {
        match self.resolve_endpoint(endpoint) {
            DeviceEndpointTarget::Cell(cell_id) => {
                DeviceEndpointRef::Cell(&device.cells[cell_id.0])
            }
            DeviceEndpointTarget::Port(port_id) => {
                DeviceEndpointRef::Port(&device.ports[port_id.0])
            }
            DeviceEndpointTarget::Unknown => DeviceEndpointRef::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DeviceDesignIndex, DeviceEndpointTarget};
    use crate::{
        bitgen::{DeviceCell, DeviceDesign, DeviceEndpoint, DevicePort},
        domain::{CellKind, EndpointKind, SiteKind},
        ir::PortDirection,
    };

    #[test]
    fn resolves_device_endpoints_to_typed_targets() {
        let device = DeviceDesign {
            ports: vec![DevicePort::new("in", PortDirection::Input, "P1").sited(
                SiteKind::Iob,
                "IOB0",
                "IO0",
                "RIGHT",
                (1, 0, 0),
            )],
            cells: vec![DeviceCell::new("u0", CellKind::Lut, "LUT4").placed(
                SiteKind::LogicSlice,
                "S0",
                "LUT0",
                "T0",
                "CENTER",
                (1, 1, 0),
            )],
            ..DeviceDesign::default()
        };
        let index = DeviceDesignIndex::build(&device);

        assert!(matches!(
            index.resolve_endpoint(&DeviceEndpoint::new(
                EndpointKind::Port,
                "in",
                "IN",
                (1, 0, 0)
            )),
            DeviceEndpointTarget::Port(_)
        ));
        assert!(matches!(
            index.resolve_endpoint(&DeviceEndpoint::new(
                EndpointKind::Cell,
                "u0",
                "O",
                (1, 1, 0)
            )),
            DeviceEndpointTarget::Cell(_)
        ));
        assert_eq!(
            index.resolve_endpoint(&DeviceEndpoint::new(
                EndpointKind::Cell,
                "ghost",
                "O",
                (0, 0, 0)
            )),
            DeviceEndpointTarget::Unknown
        );
    }
}
