use crate::{
    constraints::ClockConstraint,
    domain::PrimitiveKind,
    ir::{Design, DesignIndex, Endpoint, EndpointTarget},
    resource::CellTimingModel,
};
use std::collections::BTreeSet;

use super::{
    error::StaError,
    keys::{ArrivalMap, TimingKey, endpoint_arrival_key},
};

#[derive(Debug)]
pub(crate) struct TimingRequirements {
    pub(crate) clocks: Vec<ClockConstraint>,
    register_inputs: BTreeSet<TimingKey>,
    required_ns: Option<f64>,
    setup_ns: f64,
}

impl TimingRequirements {
    pub(crate) fn compile(
        design: &Design,
        index: &DesignIndex<'_>,
        clocks: &[ClockConstraint],
        cell_timing: &CellTimingModel,
    ) -> Result<Self, StaError> {
        let clock = match clocks {
            [] => None,
            [clock] => Some(clock),
            _ => {
                return Err(StaError::MultipleClockDomains {
                    count: clocks.len(),
                });
            }
        };
        if let Some(clock) = clock {
            validate_clock_domain(design, index, clock)?;
        }
        let register_inputs = design
            .cells
            .iter()
            .filter(|cell| matches!(cell.primitive_kind(), PrimitiveKind::FlipFlop))
            .flat_map(|cell| {
                cell.inputs
                    .iter()
                    .filter(|pin| cell.primitive_kind().is_register_data_pin(&pin.port))
                    .map(|pin| endpoint_arrival_key(index, &Endpoint::cell(&cell.name, &pin.port)))
            })
            .collect();
        let setup_ns = cell_timing.sequential.setup_ns;
        Ok(Self {
            clocks: clocks.to_vec(),
            register_inputs,
            required_ns: clock.map(|clock| clock.period_ns - setup_ns),
            setup_ns,
        })
    }

    pub(crate) fn required_ns(&self, key: &TimingKey) -> Option<f64> {
        self.register_inputs
            .contains(key)
            .then_some(self.required_ns)
            .flatten()
    }

    pub(crate) fn setup_ns(&self, key: &TimingKey) -> f64 {
        if self.register_inputs.contains(key) {
            self.setup_ns
        } else {
            0.0
        }
    }

    pub(crate) fn worst_slack_ns(&self, arrival: &ArrivalMap) -> Option<f64> {
        let required_ns = self.required_ns?;
        self.register_inputs
            .iter()
            .filter_map(|key| arrival.get(key).map(|value| required_ns - value))
            .min_by(f64::total_cmp)
    }
}

fn validate_clock_domain(
    design: &Design,
    index: &DesignIndex<'_>,
    clock: &ClockConstraint,
) -> Result<(), StaError> {
    let Some(port_id) = index.port_id(&clock.port_name) else {
        return Err(StaError::UnknownClockPort {
            clock: clock.name.clone(),
            port: clock.port_name.clone(),
        });
    };
    if !index.port(design, port_id).direction.is_input_like() {
        return Err(StaError::InvalidClockPort {
            clock: clock.name.clone(),
            port: clock.port_name.clone(),
        });
    }
    if let Some(cell) = design.cells.iter().find(|cell| cell.is_block_ram()) {
        return unsupported(cell, "block RAM");
    }
    let mut sequential_count = 0;
    for cell in design.cells.iter().filter(|cell| cell.is_sequential()) {
        if matches!(cell.primitive_kind(), PrimitiveKind::Latch) {
            return unsupported(cell, "latch");
        }
        sequential_count += 1;
        let source_port = cell
            .register_clock_net()
            .and_then(|net| source_port_for_net(design, index, net, &mut BTreeSet::new()));
        if source_port != Some(clock.port_name.as_str()) {
            return Err(StaError::UnconstrainedSequentialCell {
                cell: cell.name.clone(),
            });
        }
    }
    if sequential_count == 0 {
        return Err(StaError::UnusedClock {
            clock: clock.name.clone(),
            port: clock.port_name.clone(),
        });
    }
    Ok(())
}

fn unsupported<T>(cell: &crate::ir::Cell, kind: &str) -> Result<T, StaError> {
    Err(StaError::UnsupportedSequentialCell {
        cell: cell.name.clone(),
        kind: kind.to_string(),
    })
}

fn source_port_for_net<'a>(
    design: &'a Design,
    index: &DesignIndex<'_>,
    net_name: &'a str,
    visited: &mut BTreeSet<&'a str>,
) -> Option<&'a str> {
    if !visited.insert(net_name) {
        return None;
    }
    let driver = index.net(design, index.net_id(net_name)?).driver.as_ref()?;
    match index.resolve_endpoint(driver) {
        EndpointTarget::Port(port_id) => Some(index.port(design, port_id).name.as_str()),
        EndpointTarget::Cell(cell_id) => {
            let cell = index.cell(design, cell_id);
            if !cell.is_buffer()
                && !matches!(cell.primitive_kind(), PrimitiveKind::GlobalClockBuffer)
            {
                return None;
            }
            source_port_for_net(design, index, &cell.inputs.first()?.net, visited)
        }
        EndpointTarget::Unknown => None,
    }
}
