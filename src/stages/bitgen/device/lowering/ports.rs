use super::{DeviceCell, DeviceLowering, DevicePort, PadSiteKind, ResolvedPortSite};
use crate::{
    domain::{PinRole, PrimitiveKind, SiteKind},
    ir::{Design, DesignIndex, Port, PortId},
};

impl<'a> DeviceLowering<'a> {
    pub(super) fn materialize_ports(&mut self) {
        for port in &self.design.ports {
            let Some(port_id) = self.index.port_id(&port.name) else {
                continue;
            };
            let Some(binding) = self.resolve_port_site(port) else {
                continue;
            };
            self.push_device_port(
                port_id,
                DevicePort::new(
                    port.name.clone(),
                    port.direction.clone(),
                    binding.pin_name.clone(),
                )
                .sited(
                    binding.site_kind,
                    binding.site_name.clone(),
                    binding.tile_name.clone(),
                    binding.tile_type.clone(),
                    (binding.x, binding.y, binding.z),
                ),
            );
            if port.direction.is_input_like() {
                self.materialize_input_buffer(port_id, port, &binding);
            } else if port.direction.is_output_like() {
                self.materialize_output_buffer(port_id, port, &binding);
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
        let site_kind = pad_kind.site_kind();
        let site_name = pad
            .and_then(|pad| {
                self.cil.and_then(|cil| {
                    cil.site_name_for_kind(&pad.tile_type, pad.site_kind.site_kind(), pad.z)
                })
            })
            .unwrap_or(site_kind.as_str())
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

    fn materialize_input_buffer(
        &mut self,
        port_id: PortId,
        port: &Port,
        binding: &ResolvedPortSite,
    ) {
        let io_name = format!("$iob${}", port.name);
        let io_type = binding.pad_kind.io_type_name();
        self.bind_io_cell(
            port_id,
            DeviceCell::new(io_name.clone(), io_type)
                .placed(
                    binding.site_kind,
                    binding.site_name.clone(),
                    "PAD",
                    binding.tile_name.clone(),
                    binding.tile_type.clone(),
                    (binding.x, binding.y, binding.z),
                )
                .synthetic(),
        );

        if !is_clock_port(self.design, &self.index, port_id)
            || binding.pad_kind != PadSiteKind::GclkIob
        {
            return;
        }

        let gclk_name = format!("$gclk${}", port.name);
        let gclk_site_name = self
            .cil
            .and_then(|cil| cil.site_name_for_kind(&binding.tile_type, SiteKind::Gclk, binding.z))
            .unwrap_or("GCLK")
            .to_string();
        self.bind_gclk_cell(
            port_id,
            DeviceCell::new(gclk_name.clone(), "GCLK")
                .placed(
                    SiteKind::Gclk,
                    gclk_site_name,
                    "BUF",
                    binding.tile_name.clone(),
                    binding.tile_type.clone(),
                    (binding.x, binding.y, binding.z),
                )
                .synthetic(),
        );
    }

    fn materialize_output_buffer(
        &mut self,
        port_id: PortId,
        port: &Port,
        binding: &ResolvedPortSite,
    ) {
        let io_name = format!("$iob${}", port.name);
        self.bind_io_cell(
            port_id,
            DeviceCell::new(io_name.clone(), "IOB")
                .placed(
                    binding.site_kind,
                    binding.site_name.clone(),
                    "PAD",
                    binding.tile_name.clone(),
                    binding.tile_type.clone(),
                    (binding.x, binding.y, binding.z),
                )
                .synthetic(),
        );
    }
}

fn is_clock_port(design: &Design, index: &DesignIndex<'_>, port_id: PortId) -> bool {
    design.nets.iter().any(|net| {
        net.driver
            .as_ref()
            .is_some_and(|driver| index.port_for_endpoint(driver) == Some(port_id))
            && net.sinks.iter().any(|sink| {
                PinRole::classify_for_primitive(PrimitiveKind::FlipFlop, &sink.pin)
                    == PinRole::RegisterClock
            })
    })
}
