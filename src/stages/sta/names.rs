use crate::ir::{Cell, Design, TimingPointKind};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct TimingNames {
    cells: BTreeMap<String, String>,
    nets: BTreeMap<String, String>,
    ports: BTreeSet<String>,
}

impl TimingNames {
    pub(super) fn new(design: &Design) -> Self {
        let mut cells = BTreeMap::new();
        let mut registers = 0usize;
        let mut luts = 0usize;
        let mut others = 0usize;
        for cell in &design.cells {
            let label = if is_internal_name(&cell.name) {
                if cell.is_sequential() {
                    registers += 1;
                    format!("Register {registers}")
                } else if cell.is_lut() {
                    luts += 1;
                    format!("{} {luts}", cell.type_name)
                } else {
                    others += 1;
                    format!("{} {others}", cell.type_name)
                }
            } else {
                semantic_cell_name(cell)
            };
            cells.insert(cell.name.clone(), label);
        }

        let mut nets = BTreeMap::new();
        let mut internal_nets = 0usize;
        for net in &design.nets {
            let label = if is_internal_name(&net.name) {
                internal_nets += 1;
                format!("Net {internal_nets}")
            } else {
                normalize_name(&net.name)
            };
            nets.insert(net.name.clone(), label);
        }
        let ports = design.ports.iter().map(|port| port.name.clone()).collect();
        Self { cells, nets, ports }
    }

    pub(super) fn endpoint(&self, endpoint: &str) -> String {
        let Some((object, pin)) = endpoint.rsplit_once(':') else {
            return Self::object(endpoint);
        };
        if let Some(cell) = self.cells.get(object) {
            return format!("{cell}/{pin}");
        }
        if self.ports.contains(object) {
            return normalize_name(object);
        }
        format!("{}/{pin}", normalize_name(object))
    }

    pub(super) fn point(&self, kind: TimingPointKind, object: &str) -> String {
        match kind {
            TimingPointKind::CellArc => self
                .cells
                .get(object)
                .cloned()
                .unwrap_or_else(|| Self::object(object)),
            TimingPointKind::Net => self.net_path(object),
            TimingPointKind::ClockToQ | TimingPointKind::Endpoint | TimingPointKind::Port => {
                self.endpoint(object)
            }
            TimingPointKind::SetupCheck => "Library setup check".to_string(),
        }
    }

    fn net_path(&self, object: &str) -> String {
        let (net, target) = object
            .split_once(" -> ")
            .map_or((object, None), |(net, target)| (net, Some(target)));
        let net = self
            .nets
            .get(net)
            .cloned()
            .unwrap_or_else(|| Self::object(net));
        target.map_or_else(
            || net.clone(),
            |target| format!("{net} -> {}", self.endpoint(target)),
        )
    }

    fn object(object: &str) -> String {
        if is_internal_name(object) {
            "Internal logic".to_string()
        } else {
            normalize_name(object)
        }
    }
}

fn semantic_cell_name(cell: &Cell) -> String {
    let name = normalize_name(&cell.name);
    if cell.is_sequential() {
        let suffix = format!("_{}_Q", cell.type_name);
        if let Some(signal) = name.strip_suffix(&suffix) {
            return signal.to_string();
        }
    }
    if let Some(register) = name.strip_suffix("__d_gate_lut") {
        return format!("{register} data gate");
    }
    name
}

fn is_internal_name(name: &str) -> bool {
    name.starts_with('$')
        || ["ff.cc:", "blifparse.cc:", "proc_rom.cc:", "rtlil.cc:"]
            .iter()
            .any(|marker| name.contains(marker))
}

fn normalize_name(name: &str) -> String {
    name.strip_prefix('\\').unwrap_or(name).to_string()
}
