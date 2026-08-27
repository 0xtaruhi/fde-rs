use crate::{
    ir::{
        Cell, Design, DesignIndex, Endpoint, EndpointTarget, Net, RoutePip, RouteSegment,
        TimingDelaySource,
    },
    resource::{Arch, DelayModel},
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct DelayEstimate {
    pub(crate) delay_ns: f64,
    pub(crate) source: TimingDelaySource,
}

pub(crate) fn net_delay_to_sink_ns(
    design: &Design,
    index: &DesignIndex<'_>,
    net: &Net,
    sink: &Endpoint,
    arch: Option<&Arch>,
    delay: Option<&DelayModel>,
) -> f64 {
    net_delay_estimate(design, index, net, Some(sink), arch, delay).delay_ns
}

pub(crate) fn net_delay_estimate(
    design: &Design,
    index: &DesignIndex<'_>,
    net: &Net,
    sink: Option<&Endpoint>,
    arch: Option<&Arch>,
    delay: Option<&DelayModel>,
) -> DelayEstimate {
    if let Some(sink) = sink
        && let Some(path) = net.route_for_sink(sink)
    {
        return DelayEstimate {
            delay_ns: path.estimated_delay_ns,
            source: TimingDelaySource::RoutedRc,
        };
    }
    if !net.route.is_empty() {
        return DelayEstimate {
            delay_ns: estimate_route_delay(
                &net.route,
                arch.map_or(0.04, |arch| arch.wire_r),
                arch.map_or(0.03, |arch| arch.wire_c),
            ),
            source: TimingDelaySource::RoutedRc,
        };
    }
    if !net.route_pips.is_empty() {
        return DelayEstimate {
            delay_ns: estimate_pip_delay(
                &net.route_pips,
                arch.map_or(0.04, |arch| arch.wire_r),
                arch.map_or(0.03, |arch| arch.wire_c),
            ),
            source: TimingDelaySource::RoutedRc,
        };
    }
    let Some(driver) = &net.driver else {
        return DelayEstimate {
            delay_ns: 0.0,
            source: TimingDelaySource::Unknown,
        };
    };
    let Some(sink) = sink.or_else(|| net.sinks.first()) else {
        return DelayEstimate {
            delay_ns: 0.0,
            source: TimingDelaySource::Unknown,
        };
    };
    let dxdy = endpoint_distance(driver, sink, design, index);
    if let Some(delay) = delay {
        DelayEstimate {
            delay_ns: delay.lookup(dxdy.0, dxdy.1),
            source: TimingDelaySource::DelayTable,
        }
    } else {
        DelayEstimate {
            delay_ns: (dxdy.0 + dxdy.1) as f64 * 0.08,
            source: TimingDelaySource::GeometricEstimate,
        }
    }
}

pub(crate) fn cell_delay_estimate(cell: &Cell, model: Option<&DelayModel>) -> DelayEstimate {
    DelayEstimate {
        delay_ns: combinational_cell_delay_ns(cell, model),
        source: if model.is_some() {
            TimingDelaySource::DelayTable
        } else {
            TimingDelaySource::Constant
        },
    }
}

pub(crate) fn combinational_cell_delay_ns(cell: &Cell, model: Option<&DelayModel>) -> f64 {
    let delays = model.map_or_else(Default::default, |model| model.cell_delays);
    let input_count = cell.inputs.len() as f64;
    if cell.is_lut() {
        delays.lut_base_ns + input_count * delays.lut_per_input_ns
    } else if cell.is_buffer() {
        delays.buffer_delay_ns
    } else {
        delays.other_base_ns + input_count * delays.other_per_input_ns
    }
}

pub(crate) fn cell_input_is_functional(cell: &Cell, input_port: &str) -> bool {
    let primitive = cell.primitive_kind();
    let Some(input_index) = primitive.lut_input_index(input_port) else {
        return true;
    };
    let Some(init) = cell.property("lut_init").and_then(parse_lut_init) else {
        return true;
    };
    let input_count = primitive.lut_input_count().unwrap_or(cell.inputs.len());
    let Ok(shift) = u32::try_from(input_count) else {
        return true;
    };
    let Some(entries) = 1usize.checked_shl(shift) else {
        return true;
    };
    if input_index >= input_count || entries > 128 {
        return true;
    }
    let mask = 1usize << input_index;
    (0..entries)
        .filter(|address| address & mask == 0)
        .any(|address| ((init >> address) & 1) != ((init >> (address | mask)) & 1))
}

fn parse_lut_init(value: &str) -> Option<u128> {
    let value = value.trim();
    let digits = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))?;
    u128::from_str_radix(digits, 16).ok()
}

pub(crate) fn estimate_route_delay(route: &[RouteSegment], wire_r: f64, wire_c: f64) -> f64 {
    crate::core::ir::estimate_segment_delay_ns(route, wire_r, wire_c)
}

pub(crate) fn estimate_pip_delay(route_pips: &[RoutePip], wire_r: f64, wire_c: f64) -> f64 {
    crate::core::ir::estimate_pip_count_delay_ns(route_pips.len(), wire_r, wire_c)
}

fn endpoint_distance(
    driver: &Endpoint,
    sink: &Endpoint,
    design: &Design,
    index: &DesignIndex<'_>,
) -> (usize, usize) {
    let driver_pos = endpoint_position(driver, design, index).unwrap_or((0, 0));
    let sink_pos = endpoint_position(sink, design, index).unwrap_or((0, 0));
    (
        driver_pos.0.abs_diff(sink_pos.0),
        driver_pos.1.abs_diff(sink_pos.1),
    )
}

fn endpoint_position(
    endpoint: &Endpoint,
    design: &Design,
    index: &DesignIndex<'_>,
) -> Option<(usize, usize)> {
    match index.resolve_endpoint(endpoint) {
        EndpointTarget::Cell(cell_id) => index.cluster_for_cell(cell_id).and_then(|cluster_id| {
            let cluster = index.cluster(design, cluster_id);
            Some((cluster.x?, cluster.y?))
        }),
        EndpointTarget::Port(port_id) => {
            let port = index.port(design, port_id);
            Some((port.x?, port.y?))
        }
        EndpointTarget::Unknown => None,
    }
}
