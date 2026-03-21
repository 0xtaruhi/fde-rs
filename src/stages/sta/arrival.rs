use crate::{
    ir::Design,
    resource::{Arch, DelayModel},
};

use super::{
    delay::{intrinsic_cell_delay_ns, net_delay_ns},
    error::StaError,
    keys::{ArrivalMap, cell_arrival_key, endpoint_arrival_key, net_arrival_key, port_arrival_key},
};

pub(crate) fn compute_arrivals(
    design: &Design,
    arch: Option<&Arch>,
    delay: Option<&DelayModel>,
) -> Result<ArrivalMap, StaError> {
    let mut arrival = ArrivalMap::new();
    for port in &design.ports {
        if port.direction.is_input_like() {
            arrival.insert(port_arrival_key(&port.name), 0.0);
        }
    }
    for cell in &design.cells {
        if cell.is_constant_source() {
            for output in &cell.outputs {
                arrival.insert(cell_arrival_key(&cell.name, &output.port), 0.0);
            }
        }
        if cell.is_sequential() {
            for output in &cell.outputs {
                arrival.insert(cell_arrival_key(&cell.name, &output.port), 0.2);
            }
        }
    }

    let mut changed = true;
    for _ in 0..design.cells.len().max(1) * 2 {
        if !changed {
            break;
        }
        changed = false;
        for cell in &design.cells {
            if cell.is_sequential() {
                continue;
            }
            let mut input_arrival: f64 = 0.0;
            for input in &cell.inputs {
                let Some(net) = design.net_lookup(&input.net) else {
                    continue;
                };
                let driver_key = net
                    .driver
                    .as_ref()
                    .map(endpoint_arrival_key)
                    .unwrap_or_else(|| net_arrival_key(&net.name));
                let src_arrival = arrival.get(&driver_key).copied().unwrap_or(0.0);
                let net_delay = net_delay_ns(design, net, arch, delay);
                input_arrival = input_arrival.max(src_arrival + net_delay);
            }
            let output_arrival = input_arrival + intrinsic_cell_delay_ns(cell);
            for output in &cell.outputs {
                let key = cell_arrival_key(&cell.name, &output.port);
                if output_arrival > *arrival.get(&key).unwrap_or(&-1.0) {
                    arrival.insert(key, output_arrival);
                    changed = true;
                }
            }
        }
    }

    for net in &design.nets {
        let driver_arrival = net
            .driver
            .as_ref()
            .map(endpoint_arrival_key)
            .and_then(|key| arrival.get(&key).copied())
            .unwrap_or(0.0);
        let delay_ns = net_delay_ns(design, net, arch, delay);
        for sink in &net.sinks {
            arrival.insert(endpoint_arrival_key(sink), driver_arrival + delay_ns);
        }
    }

    validate_arrivals(&arrival)?;
    Ok(arrival)
}

fn validate_arrivals(arrival: &ArrivalMap) -> Result<(), StaError> {
    for (key, value) in arrival {
        if !value.is_finite() {
            return Err(StaError::NonFiniteArrival {
                key: key.clone(),
                value: *value,
            });
        }
    }
    Ok(())
}
