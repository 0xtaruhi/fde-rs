use super::{DeviceCell, DeviceLowering, DevicePort, PadSiteKind, ResolvedPortSite};
use crate::{
    domain::{PinRole, PrimitiveKind},
    ir::{Design, Port},
};

impl<'a> DeviceLowering<'a> {
    pub(super) fn materialize_ports(&mut self) {
        for port in &self.design.ports {
            let Some(binding) = self.resolve_port_site(port) else {
                continue;
            };
            self.device.ports.push(DevicePort {
                port_name: port.name.clone(),
                direction: port.direction.clone(),
                pin_name: binding.pin_name.clone(),
                site_kind: binding.site_kind.clone(),
                site_name: binding.site_name.clone(),
                tile_name: binding.tile_name.clone(),
                tile_type: binding.tile_type.clone(),
                x: binding.x,
                y: binding.y,
                z: binding.z,
            });
            if port.direction.is_input_like() {
                self.materialize_input_buffer(port, &binding);
            } else if port.direction.is_output_like() {
                self.materialize_output_buffer(port, &binding);
            }
        }
    }

    fn resolve_port_site(&self, port: &Port) -> Option<ResolvedPortSite> {
        let pin_name = port.pin.clone()?;
        let pad = self.arch.pad(&pin_name);
        let pad_kind = pad.map(|pad| pad.site_kind).unwrap_or(PadSiteKind::Iob);
        let x = port.x.unwrap_or(0);
        let y = port.y.unwrap_or(0);
        let z = pad.map(|pad| pad.z).unwrap_or(0);
        let tile_name = pad.map(|pad| pad.tile_name.clone()).unwrap_or_default();
        let tile_type = pad.map(|pad| pad.tile_type.clone()).unwrap_or_default();
        let site_kind = pad_kind.as_str().to_string();
        let site_name = pad
            .and_then(|pad| {
                self.cil.and_then(|cil| {
                    cil.site_name_for_slot(&pad.tile_type, pad.site_kind.as_str(), pad.z)
                })
            })
            .unwrap_or(&site_kind)
            .to_string();
        Some(ResolvedPortSite {
            pin_name,
            site_kind,
            site_name,
            tile_name,
            tile_type,
            x,
            y,
            z,
            pad_kind,
        })
    }

    fn materialize_input_buffer(&mut self, port: &Port, binding: &ResolvedPortSite) {
        let io_name = format!("$iob${}", port.name);
        let io_type = match binding.pad_kind {
            PadSiteKind::Iob => "IOB",
            PadSiteKind::GclkIob => "GCLKIOB",
        };
        self.device.cells.push(DeviceCell {
            cell_name: io_name.clone(),
            type_name: io_type.to_string(),
            properties: Vec::new(),
            site_kind: binding.site_kind.clone(),
            site_name: binding.site_name.clone(),
            bel: "PAD".to_string(),
            tile_name: binding.tile_name.clone(),
            tile_type: binding.tile_type.clone(),
            x: binding.x,
            y: binding.y,
            z: binding.z,
            cluster_name: None,
            synthetic: true,
        });
        self.port_to_io.insert(port.name.clone(), io_name.clone());

        if !is_clock_port(self.design, &port.name) || binding.pad_kind != PadSiteKind::GclkIob {
            return;
        }

        let gclk_name = format!("$gclk${}", port.name);
        let gclk_site_name = self
            .cil
            .and_then(|cil| cil.site_name_for_slot(&binding.tile_type, "GCLK", binding.z))
            .unwrap_or("GCLK")
            .to_string();
        self.device.cells.push(DeviceCell {
            cell_name: gclk_name.clone(),
            type_name: "GCLK".to_string(),
            properties: Vec::new(),
            site_kind: "GCLK".to_string(),
            site_name: gclk_site_name,
            bel: "BUF".to_string(),
            tile_name: binding.tile_name.clone(),
            tile_type: binding.tile_type.clone(),
            x: binding.x,
            y: binding.y,
            z: binding.z,
            cluster_name: None,
            synthetic: true,
        });
        self.port_to_gclk.insert(port.name.clone(), gclk_name);
    }

    fn materialize_output_buffer(&mut self, port: &Port, binding: &ResolvedPortSite) {
        let io_name = format!("$iob${}", port.name);
        self.device.cells.push(DeviceCell {
            cell_name: io_name.clone(),
            type_name: "IOB".to_string(),
            properties: Vec::new(),
            site_kind: binding.site_kind.clone(),
            site_name: binding.site_name.clone(),
            bel: "PAD".to_string(),
            tile_name: binding.tile_name.clone(),
            tile_type: binding.tile_type.clone(),
            x: binding.x,
            y: binding.y,
            z: binding.z,
            cluster_name: None,
            synthetic: true,
        });
        self.port_to_io.insert(port.name.clone(), io_name);
    }
}

fn is_clock_port(design: &Design, port_name: &str) -> bool {
    design.nets.iter().any(|net| {
        net.driver
            .as_ref()
            .is_some_and(|driver| driver.is_port() && driver.name == port_name)
            && net.sinks.iter().any(|sink| {
                PinRole::classify_for_primitive(PrimitiveKind::FlipFlop, &sink.pin)
                    == PinRole::RegisterClock
            })
    })
}
