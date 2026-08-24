use crate::route::{
    types::RouteNode,
    wire::{
        WireBounds, route_node_base_cost, route_node_class_for_wire, tile_distance,
        wire_bounds_for_wire,
    },
};

use super::{
    policy::node_has_successors,
    router::{RouteSinkContext, SinkRouteSpec},
};

/// Timing-driven discount ceiling: a maximally critical net pays at least
/// 75% of the base node cost, keeping costs positive and bounded.
const MAX_CRITICALITY_DISCOUNT: f64 = 0.25;

pub(super) fn route_transition_cost(
    context: &RouteSinkContext<'_>,
    spec: &SinkRouteSpec<'_>,
    _current: &RouteNode,
    neighbor: &RouteNode,
    local_arc: Option<usize>,
) -> usize {
    if local_arc.is_none() {
        return 0;
    }
    let base = route_node_cost(context, neighbor);
    discount_for_criticality(base, spec.criticality)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn discount_for_criticality(base: usize, criticality: f64) -> usize {
    if base == 0 || !criticality.is_finite() || criticality <= f64::EPSILON {
        return base;
    }
    let factor = 1.0 - MAX_CRITICALITY_DISCOUNT * criticality.clamp(0.0, 1.0);
    let discounted = (base as f64) * factor;
    debug_assert!(discounted.is_finite() && discounted >= 0.0);
    discounted.round() as usize
}

pub(super) fn route_heuristic(
    context: &RouteSinkContext<'_>,
    node: &RouteNode,
    sink_x: usize,
    sink_y: usize,
) -> usize {
    let Some(bounds) = context.stitched_components.bounds(node) else {
        if let Some(bounds) =
            wire_bounds_for_wire(context.arch, node.x, node.y, context.wires, node.wire)
        {
            return axis_distance(sink_x, bounds.min_x, bounds.max_x)
                + axis_distance(sink_y, bounds.min_y, bounds.max_y);
        }
        return tile_distance(node.x, node.y, sink_x, sink_y);
    };

    axis_distance(sink_x, bounds.min_x, bounds.max_x)
        + axis_distance(sink_y, bounds.min_y, bounds.max_y)
}

fn route_node_cost(context: &RouteSinkContext<'_>, node: &RouteNode) -> usize {
    let bounds = context
        .stitched_components
        .bounds(node)
        .map(|bounds| WireBounds {
            min_x: bounds.min_x,
            max_x: bounds.max_x,
            min_y: bounds.min_y,
            max_y: bounds.max_y,
        })
        .or_else(|| wire_bounds_for_wire(context.arch, node.x, node.y, context.wires, node.wire));
    let class = route_node_class_for_wire(
        context.wires,
        node.wire,
        bounds,
        node_has_successors(context, node),
    );
    route_node_base_cost(class)
}

fn axis_distance(value: usize, min: usize, max: usize) -> usize {
    if value < min {
        min - value
    } else {
        value.saturating_sub(max)
    }
}

#[cfg(test)]
mod tests {
    use super::discount_for_criticality;

    #[test]
    fn discounts_scale_monotonically_and_stay_positive() {
        assert_eq!(discount_for_criticality(100, 0.0), 100);
        assert_eq!(discount_for_criticality(0, 1.0), 0);
        assert_eq!(discount_for_criticality(4, 1.0), 3);

        // Monotonically non-increasing in criticality, never below 75%.
        let mut previous = 100usize;
        for step in 1..=10usize {
            let crit = step as f64 / 10.0;
            let discounted = discount_for_criticality(100, crit);
            assert!(discounted <= previous);
            assert!(discounted >= 75);
            previous = discounted;
        }
    }
}
