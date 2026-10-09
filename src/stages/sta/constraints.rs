use crate::{
    constraints::{ClockConstraint, ClockUncertaintyConstraint, IoDelayConstraint},
    domain::{BlockRamPin, BlockRamPortSide, PrimitiveKind},
    ir::{Cell, Design, DesignIndex, Endpoint, EndpointTarget, PortId},
    resource::CellTimingModel,
};
use std::collections::{BTreeMap, BTreeSet};

use super::{
    error::StaError,
    keys::{ArrivalMap, TimingKey, endpoint_arrival_key},
};

#[derive(Debug)]
pub(crate) struct TimingRequirements {
    pub(crate) clocks: Vec<ClockConstraint>,
    /// Synchronous capture endpoints and their library setup time.
    register_inputs: BTreeMap<TimingKey, f64>,
    endpoint_requirements: BTreeMap<TimingKey, EndpointTimingRequirement>,
    /// Constrained clock that launches each synchronous output pin.
    launch_clocks: BTreeMap<TimingKey, String>,
    input_delays: BTreeMap<PortId, f64>,
    constrained_primary_outputs: usize,
    clock_uncertainties: BTreeMap<String, f64>,
    block_ram_endpoints: usize,
    block_ram_launch_pins: usize,
    block_ram_timing_modeled: bool,
}

/// One clock domain of a synchronous cell: a flip-flop, or one port of a
/// block RAM.
struct SyncDomain<'a> {
    clock_net: Option<&'a str>,
    capture_pins: Vec<&'a str>,
    launch_pins: Vec<&'a str>,
    setup_ns: f64,
    block_ram: bool,
}

#[derive(Debug, Clone)]
struct EndpointTimingRequirement {
    clock_name: String,
    required_ns: f64,
}

impl TimingRequirements {
    pub(crate) fn compile(
        design: &Design,
        index: &DesignIndex<'_>,
        clocks: &[ClockConstraint],
        input_delays: &[IoDelayConstraint],
        output_delays: &[IoDelayConstraint],
        clock_uncertainties: &[ClockUncertaintyConstraint],
        cell_timing: &CellTimingModel,
    ) -> Result<Self, StaError> {
        validate_clock_ports(design, index, clocks)?;
        validate_io_delays(design, index, clocks, input_delays, true)?;
        validate_io_delays(design, index, clocks, output_delays, false)?;
        let clock_uncertainties = compile_clock_uncertainties(clocks, clock_uncertainties)?;
        let mut register_inputs = BTreeMap::new();
        let mut endpoint_requirements = BTreeMap::new();
        let mut launch_clocks = BTreeMap::new();
        let mut used_clocks = BTreeSet::new();
        let mut block_ram_endpoints = 0usize;
        let mut block_ram_launch_pins = 0usize;
        for cell in design.cells.iter().filter(|cell| cell.is_timing_boundary()) {
            if matches!(cell.primitive_kind(), PrimitiveKind::Latch) {
                return unsupported(cell, "latch");
            }
            for domain in sync_domains(cell, cell_timing) {
                if domain.block_ram {
                    block_ram_launch_pins += domain.launch_pins.len();
                }
                let clock = if clocks.is_empty() {
                    None
                } else {
                    let source_port = domain.clock_net.and_then(|net| {
                        source_port_for_net(design, index, net, &mut BTreeSet::new())
                    });
                    let Some(clock) = clocks
                        .iter()
                        .find(|clock| Some(clock.port_name.as_str()) == source_port)
                    else {
                        return Err(StaError::UnconstrainedSequentialCell {
                            cell: cell.name.clone(),
                        });
                    };
                    used_clocks.insert(clock.name.clone());
                    Some(clock)
                };
                if let Some(clock) = clock {
                    for pin in &domain.launch_pins {
                        launch_clocks.insert(
                            endpoint_arrival_key(index, &Endpoint::cell(&cell.name, *pin)),
                            clock.name.clone(),
                        );
                    }
                }
                for pin in &domain.capture_pins {
                    let key = endpoint_arrival_key(index, &Endpoint::cell(&cell.name, *pin));
                    register_inputs.insert(key.clone(), domain.setup_ns);
                    if domain.block_ram {
                        block_ram_endpoints += 1;
                    }
                    if let Some(clock) = clock {
                        endpoint_requirements.insert(
                            key,
                            EndpointTimingRequirement {
                                clock_name: clock.name.clone(),
                                required_ns: clock.period_ns
                                    - domain.setup_ns
                                    - clock_uncertainties.get(&clock.name).copied().unwrap_or(0.0),
                            },
                        );
                    }
                }
            }
        }
        used_clocks.extend(
            input_delays
                .iter()
                .chain(output_delays)
                .map(|delay| delay.clock_name.clone()),
        );
        if let Some(clock) = clocks
            .iter()
            .find(|clock| !used_clocks.contains(&clock.name))
        {
            return Err(StaError::UnusedClock {
                clock: clock.name.clone(),
                port: clock.port_name.clone(),
            });
        }
        for delay in output_delays {
            let clock = clocks
                .iter()
                .find(|clock| clock.name == delay.clock_name)
                .ok_or_else(|| StaError::UnknownTimingClock {
                    clock: delay.clock_name.clone(),
                })?;
            let required_ns = clock.period_ns
                - delay.delay_ns
                - clock_uncertainties.get(&clock.name).copied().unwrap_or(0.0);
            let port_id =
                index
                    .port_id(&delay.port_name)
                    .ok_or_else(|| StaError::UnknownIoDelayPort {
                        kind: "output".to_string(),
                        port: delay.port_name.clone(),
                    })?;
            let mut matched = false;
            for net in &design.nets {
                for sink in &net.sinks {
                    if index.resolve_endpoint(sink) == EndpointTarget::Port(port_id) {
                        endpoint_requirements.insert(
                            endpoint_arrival_key(index, sink),
                            EndpointTimingRequirement {
                                clock_name: clock.name.clone(),
                                required_ns,
                            },
                        );
                        matched = true;
                    }
                }
            }
            if !matched {
                endpoint_requirements.insert(
                    endpoint_arrival_key(
                        index,
                        &Endpoint::port(&delay.port_name, &delay.port_name),
                    ),
                    EndpointTimingRequirement {
                        clock_name: clock.name.clone(),
                        required_ns,
                    },
                );
            }
        }
        let input_delays = input_delays
            .iter()
            .filter_map(|delay| {
                index
                    .port_id(&delay.port_name)
                    .map(|port_id| (port_id, delay.delay_ns))
            })
            .collect();
        Ok(Self {
            clocks: clocks.to_vec(),
            register_inputs,
            endpoint_requirements,
            launch_clocks,
            input_delays,
            constrained_primary_outputs: output_delays.len(),
            clock_uncertainties,
            block_ram_endpoints,
            block_ram_launch_pins,
            block_ram_timing_modeled: cell_timing.block_ram.is_some(),
        })
    }

    pub(crate) fn required_ns(&self, key: &TimingKey) -> Option<f64> {
        self.endpoint_requirements
            .get(key)
            .map(|requirement| requirement.required_ns)
    }

    pub(crate) fn setup_ns(&self, key: &TimingKey) -> f64 {
        self.register_inputs.get(key).copied().unwrap_or(0.0)
    }

    pub(crate) fn is_register_input(&self, key: &TimingKey) -> bool {
        self.register_inputs.contains_key(key)
    }

    pub(crate) fn block_ram_endpoint_count(&self) -> usize {
        self.block_ram_endpoints
    }

    /// Block RAM endpoints analyzed without library timing arcs. Their slacks
    /// are optimistic, so they must never let the design report MET.
    pub(crate) fn unmodeled_block_ram_endpoint_count(&self) -> usize {
        if self.block_ram_timing_modeled {
            0
        } else {
            self.block_ram_endpoints
        }
    }

    pub(crate) fn register_endpoint_count(&self) -> usize {
        self.register_inputs.len()
    }

    pub(crate) fn block_ram_launch_pin_count(&self) -> usize {
        self.block_ram_launch_pins
    }

    pub(crate) fn unmodeled_block_ram_launch_pin_count(&self) -> usize {
        if self.block_ram_timing_modeled {
            0
        } else {
            self.block_ram_launch_pins
        }
    }

    pub(crate) fn constrained_register_endpoint_count(&self) -> usize {
        self.register_inputs
            .keys()
            .filter(|key| self.endpoint_requirements.contains_key(*key))
            .count()
    }

    pub(crate) fn constrained_primary_input_count(&self) -> usize {
        self.input_delays.len()
    }

    pub(crate) fn constrained_primary_output_count(&self) -> usize {
        self.constrained_primary_outputs
    }

    pub(crate) fn input_delay_ns(&self, port_id: PortId) -> f64 {
        self.input_delays.get(&port_id).copied().unwrap_or(0.0)
    }

    pub(crate) fn clock_uncertainty_ns(&self, clock_name: &str) -> f64 {
        self.clock_uncertainties
            .get(clock_name)
            .copied()
            .unwrap_or(0.0)
    }

    pub(crate) fn is_clock_port(&self, port_name: &str) -> bool {
        self.clocks.iter().any(|clock| clock.port_name == port_name)
    }

    pub(crate) fn clock_name_for_endpoint(&self, key: &TimingKey) -> Option<&str> {
        self.endpoint_requirements
            .get(key)
            .map(|requirement| requirement.clock_name.as_str())
    }

    pub(crate) fn clock_name_for_launch(&self, key: &TimingKey) -> Option<&str> {
        self.launch_clocks.get(key).map(String::as_str)
    }

    pub(crate) fn register_count_for_clock(&self, clock_name: &str) -> usize {
        self.register_inputs
            .keys()
            .filter_map(|key| self.endpoint_requirements.get(key))
            .filter(|requirement| requirement.clock_name == clock_name)
            .count()
    }

    pub(crate) fn slack_ns(&self, key: &TimingKey, arrival_ns: f64) -> Option<f64> {
        self.required_ns(key)
            .map(|required_ns| required_ns - arrival_ns)
    }

    pub(crate) fn constrained_endpoint_slacks<'a>(
        &'a self,
        arrival: &'a ArrivalMap,
    ) -> impl Iterator<Item = (&'a TimingKey, f64)> + 'a {
        self.endpoint_requirements.keys().filter_map(|key| {
            let arrival_ns = arrival.get(key).copied()?;
            Some((key, self.slack_ns(key, arrival_ns)?))
        })
    }
}

fn compile_clock_uncertainties(
    clocks: &[ClockConstraint],
    uncertainties: &[ClockUncertaintyConstraint],
) -> Result<BTreeMap<String, f64>, StaError> {
    let mut result = BTreeMap::new();
    for uncertainty in uncertainties {
        if !clocks
            .iter()
            .any(|clock| clock.name == uncertainty.clock_name)
        {
            return Err(StaError::UnknownTimingClock {
                clock: uncertainty.clock_name.clone(),
            });
        }
        result.insert(uncertainty.clock_name.clone(), uncertainty.setup_ns);
    }
    Ok(result)
}

fn validate_io_delays(
    design: &Design,
    index: &DesignIndex<'_>,
    clocks: &[ClockConstraint],
    delays: &[IoDelayConstraint],
    input: bool,
) -> Result<(), StaError> {
    let kind = if input { "input" } else { "output" };
    let expected = kind;
    let mut seen_ports = BTreeSet::new();
    for delay in delays {
        if !clocks.iter().any(|clock| clock.name == delay.clock_name) {
            return Err(StaError::UnknownTimingClock {
                clock: delay.clock_name.clone(),
            });
        }
        let Some(port_id) = index.port_id(&delay.port_name) else {
            return Err(StaError::UnknownIoDelayPort {
                kind: kind.to_string(),
                port: delay.port_name.clone(),
            });
        };
        let port = index.port(design, port_id);
        let valid_direction = if input {
            port.direction.is_input_like()
        } else {
            port.direction.is_output_like()
        };
        if !valid_direction {
            return Err(StaError::InvalidIoDelayPort {
                kind: kind.to_string(),
                port: delay.port_name.clone(),
                expected: expected.to_string(),
            });
        }
        if !seen_ports.insert(delay.port_name.as_str()) {
            return Err(StaError::InvalidIoDelayPort {
                kind: kind.to_string(),
                port: delay.port_name.clone(),
                expected: "uniquely constrained".to_string(),
            });
        }
    }
    Ok(())
}

fn validate_clock_ports(
    design: &Design,
    index: &DesignIndex<'_>,
    clocks: &[ClockConstraint],
) -> Result<(), StaError> {
    for clock in clocks {
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
    }
    Ok(())
}

fn sync_domains<'a>(cell: &'a Cell, cell_timing: &CellTimingModel) -> Vec<SyncDomain<'a>> {
    if !cell.is_block_ram() {
        let kind = cell.primitive_kind();
        return vec![SyncDomain {
            clock_net: cell.register_clock_net(),
            capture_pins: cell
                .inputs
                .iter()
                .filter(|pin| kind.is_register_data_pin(&pin.port))
                .map(|pin| pin.port.as_str())
                .collect(),
            launch_pins: cell.outputs.iter().map(|pin| pin.port.as_str()).collect(),
            setup_ns: cell_timing.sequential.setup_ns,
            block_ram: false,
        }];
    }

    let setup_ns = cell_timing.block_ram.map_or(0.0, |timing| timing.setup_ns);
    [BlockRamPortSide::A, BlockRamPortSide::B]
        .into_iter()
        .filter_map(|side| {
            let on_side = |port: &str, select: fn(BlockRamPin) -> bool| {
                BlockRamPin::parse(port).is_some_and(|pin| pin.side() == side && select(pin))
            };
            // A port whose clock pin is unconnected is unused; its tied-off
            // inputs are not timing endpoints.
            let clock_net = cell
                .inputs
                .iter()
                .find(|pin| on_side(&pin.port, BlockRamPin::is_clock))
                .map(|pin| pin.net.as_str())?;
            Some(SyncDomain {
                clock_net: Some(clock_net),
                capture_pins: cell
                    .inputs
                    .iter()
                    .filter(|pin| on_side(&pin.port, BlockRamPin::is_synchronous_input))
                    .map(|pin| pin.port.as_str())
                    .collect(),
                launch_pins: cell
                    .outputs
                    .iter()
                    .filter(|pin| on_side(&pin.port, BlockRamPin::is_data_output))
                    .map(|pin| pin.port.as_str())
                    .collect(),
                setup_ns,
                block_ram: true,
            })
        })
        .collect()
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
