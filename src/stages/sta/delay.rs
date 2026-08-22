use crate::{
    ir::{Cell, Design, DesignIndex, Endpoint, EndpointTarget, Net, RoutePip, RouteSegment},
    resource::{Arch, DelayModel},
};

pub(crate) fn net_delay_ns(
    design: &Design,
    index: &DesignIndex<'_>,
    net: &Net,
    arch: Option<&Arch>,
    delay: Option<&DelayModel>,
) -> f64 {
    if !net.route.is_empty() {
        return estimate_route_delay(
            &net.route,
            arch.map_or(0.04, |arch| arch.wire_r),
            arch.map_or(0.03, |arch| arch.wire_c),
        );
    }
    if !net.route_pips.is_empty() {
        return estimate_pip_delay(
            &net.route_pips,
            arch.map_or(0.04, |arch| arch.wire_r),
            arch.map_or(0.03, |arch| arch.wire_c),
        );
    }
    let Some(driver) = &net.driver else {
        return 0.0;
    };
    let Some(sink) = net.sinks.first() else {
        return 0.0;
    };
    let dxdy = endpoint_distance(driver, sink, design, index);
    if let Some(delay) = delay {
        delay.lookup(dxdy.0, dxdy.1)
    } else {
        (dxdy.0 + dxdy.1) as f64 * 0.08
    }
}

pub(crate) fn intrinsic_cell_delay_ns(cell: &Cell) -> f64 {
    if cell.is_lut() {
        0.15 + cell.inputs.len() as f64 * 0.04
    } else if cell.is_buffer() {
        0.04
    } else if cell.is_sequential() {
        0.1
    } else {
        0.08 + cell.inputs.len() as f64 * 0.02
    }
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
