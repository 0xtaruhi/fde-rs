use crate::resource::Arch;
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, VecDeque},
};

#[cfg(test)]
use std::collections::BTreeMap;

#[cfg(test)]
use super::EdgeKey;
use super::{EdgeArray, GridPoint, canonical_edge};
use crate::route::cost::SearchProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Axis {
    Start,
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct SearchKey {
    point: GridPoint,
    axis: Axis,
}

impl Axis {
    const COUNT: usize = 3;

    fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Start,
            1 => Self::Horizontal,
            _ => Self::Vertical,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct QueueState {
    priority: f64,
    cost: f64,
    state_index: usize,
    order_index: usize,
}

impl Eq for QueueState {}

impl PartialEq for QueueState {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
            && self.cost == other.cost
            && self.state_index == other.state_index
            && self.order_index == other.order_index
    }
}

impl Ord for QueueState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .priority
            .total_cmp(&self.priority)
            .then_with(|| self.order_index.cmp(&other.order_index))
    }
}

impl PartialOrd for QueueState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub(crate) fn bfs_to_tree(
    start: GridPoint,
    tree_mask: &[bool],
    arch: &Arch,
) -> Option<Vec<GridPoint>> {
    let grid_len = arch.width.saturating_mul(arch.height);
    if grid_len == 0 {
        return None;
    }

    let start_index = point_index(start, arch);
    if tree_mask.get(start_index).copied().unwrap_or(false) {
        return Some(vec![start]);
    }

    let mut queue = VecDeque::from([start_index]);
    let mut parent = vec![None; grid_len];
    let mut seen = vec![false; grid_len];
    if let Some(slot) = seen.get_mut(start_index) {
        *slot = true;
    }

    while let Some(current_index) = queue.pop_front() {
        if tree_mask.get(current_index).copied().unwrap_or(false) {
            return Some(reconstruct_point_path(current_index, &parent, arch));
        }
        let point = point_from_index(current_index, arch);
        for_each_neighbor(point, arch, |neighbor| {
            let neighbor_index = point_index(neighbor, arch);
            if !seen.get(neighbor_index).copied().unwrap_or(true) {
                if let Some(slot) = seen.get_mut(neighbor_index) {
                    *slot = true;
                }
                if let Some(slot) = parent.get_mut(neighbor_index) {
                    *slot = Some(current_index);
                }
                queue.push_back(neighbor_index);
            }
        });
    }
    None
}

#[cfg(test)]
pub(crate) fn astar_to_tree(
    start: GridPoint,
    tree_mask: &[bool],
    tree_distance: &[usize],
    arch: &Arch,
    usage: &BTreeMap<EdgeKey, usize>,
    history: &BTreeMap<EdgeKey, f64>,
    profile: SearchProfile,
) -> Option<Vec<GridPoint>> {
    let usage_dense = EdgeArray::<usize>::from_sparse(arch, usage);
    let history_dense = EdgeArray::<f64>::from_sparse(arch, history);
    astar_to_tree_dense(
        start,
        tree_mask,
        tree_distance,
        arch,
        &usage_dense,
        &history_dense,
        profile,
    )
}

pub(super) fn astar_to_tree_dense(
    start: GridPoint,
    tree_mask: &[bool],
    tree_distance: &[usize],
    arch: &Arch,
    usage: &EdgeArray<usize>,
    history: &EdgeArray<f64>,
    profile: SearchProfile,
) -> Option<Vec<GridPoint>> {
    let grid_len = arch.width.saturating_mul(arch.height);
    if grid_len == 0 {
        return None;
    }

    let start_point_index = point_index(start, arch);
    if tree_mask.get(start_point_index).copied().unwrap_or(false) {
        return Some(vec![start]);
    }

    let start_key = SearchKey {
        point: start,
        axis: Axis::Start,
    };
    let state_count = grid_len.saturating_mul(Axis::COUNT);
    let start_state_index = search_index(start_key, arch);
    let mut frontier = BinaryHeap::new();
    frontier.push(QueueState {
        priority: 0.0,
        cost: 0.0,
        state_index: start_state_index,
        order_index: search_order_index(start_key, arch),
    });
    let mut best_cost = vec![f64::INFINITY; state_count];
    if let Some(slot) = best_cost.get_mut(start_state_index) {
        *slot = 0.0;
    }
    let mut parent = vec![None; state_count];

    while let Some(state) = frontier.pop() {
        if state.cost > *best_cost.get(state.state_index).unwrap_or(&f64::INFINITY) + 1e-9 {
            continue;
        }
        let key = search_key_from_index(state.state_index, arch);
        if tree_mask
            .get(point_index(key.point, arch))
            .copied()
            .unwrap_or(false)
        {
            return Some(reconstruct_search_path(state.state_index, &parent, arch));
        }
        let current_cost = state.cost;
        for_each_neighbor(key.point, arch, |neighbor| {
            let edge = canonical_edge(key.point, neighbor);
            let next_axis = if neighbor.x != key.point.x {
                Axis::Horizontal
            } else {
                Axis::Vertical
            };
            let bend_penalty = if matches!(key.axis, Axis::Start) || key.axis == next_axis {
                0.0
            } else {
                profile.bend_penalty
            };
            let present = usage.get(edge.0, edge.1).copied().unwrap_or(0) as f64;
            let capacity = arch.edge_capacity(key.point.into(), neighbor.into()).max(1) as f64;
            let next_load = present + 1.0;
            let utilization = next_load / capacity;
            let overflow = (next_load - capacity).max(0.0);
            let present_cost =
                utilization * utilization * 0.35 + overflow * (1.0 + overflow / capacity);
            let history_cost = history.get(edge.0, edge.1).copied().unwrap_or(0.0);
            let step_cost = 1.0
                + present_cost * profile.present_factor
                + history_cost * profile.history_factor
                + bend_penalty;
            let next_cost = current_cost + step_cost;
            let next_key = SearchKey {
                point: neighbor,
                axis: next_axis,
            };
            let next_state_index = search_index(next_key, arch);
            if next_cost + 1e-9 < *best_cost.get(next_state_index).unwrap_or(&f64::INFINITY) {
                if let Some(slot) = best_cost.get_mut(next_state_index) {
                    *slot = next_cost;
                }
                if let Some(slot) = parent.get_mut(next_state_index) {
                    *slot = Some(state.state_index);
                }
                frontier.push(QueueState {
                    priority: next_cost
                        + profile.heuristic_factor
                            * tree_distance
                                .get(point_index(neighbor, arch))
                                .copied()
                                .unwrap_or(0) as f64,
                    cost: next_cost,
                    state_index: next_state_index,
                    order_index: search_order_index(next_key, arch),
                });
            }
        });
    }

    None
}

pub(crate) fn tree_distance_field(tree_points: &[GridPoint], arch: &Arch) -> Vec<usize> {
    let grid_len = arch.width.saturating_mul(arch.height);
    if grid_len == 0 {
        return Vec::new();
    }

    let mut distance = vec![usize::MAX; grid_len];
    let mut queue = VecDeque::new();
    for point in tree_points {
        let index = point_index(*point, arch);
        if distance.get(index).copied().unwrap_or(usize::MAX) == 0 {
            continue;
        }
        if let Some(slot) = distance.get_mut(index) {
            *slot = 0;
        }
        queue.push_back(*point);
    }

    while let Some(point) = queue.pop_front() {
        let base_index = point_index(point, arch);
        let next_distance = distance
            .get(base_index)
            .copied()
            .unwrap_or(usize::MAX)
            .saturating_add(1);
        for_each_neighbor(point, arch, |neighbor| {
            let neighbor_index = point_index(neighbor, arch);
            if next_distance < distance.get(neighbor_index).copied().unwrap_or(usize::MAX) {
                if let Some(slot) = distance.get_mut(neighbor_index) {
                    *slot = next_distance;
                }
                queue.push_back(neighbor);
            }
        });
    }

    distance
}

fn reconstruct_point_path(
    mut current_index: usize,
    parent: &[Option<usize>],
    arch: &Arch,
) -> Vec<GridPoint> {
    let mut path = Vec::new();
    loop {
        path.push(point_from_index(current_index, arch));
        match parent.get(current_index).copied().flatten() {
            Some(next) => current_index = next,
            None => break,
        }
    }
    path.reverse();
    path
}

fn reconstruct_search_path(
    mut current_index: usize,
    parent: &[Option<usize>],
    arch: &Arch,
) -> Vec<GridPoint> {
    let mut path = Vec::new();
    let mut last_point = None;
    loop {
        let key = search_key_from_index(current_index, arch);
        if last_point != Some(key.point) {
            path.push(key.point);
            last_point = Some(key.point);
        }
        match parent.get(current_index).copied().flatten() {
            Some(next) => current_index = next,
            None => break,
        }
    }
    path.reverse();
    path
}

fn search_index(key: SearchKey, arch: &Arch) -> usize {
    point_index(key.point, arch) * Axis::COUNT + key.axis as usize
}

fn search_order_index(key: SearchKey, arch: &Arch) -> usize {
    point_index(key.point, arch)
}

fn search_key_from_index(index: usize, arch: &Arch) -> SearchKey {
    let point_index = index / Axis::COUNT;
    let axis_index = index % Axis::COUNT;
    SearchKey {
        point: point_from_index(point_index, arch),
        axis: Axis::from_index(axis_index),
    }
}

fn point_index(point: GridPoint, arch: &Arch) -> usize {
    point.y * arch.width + point.x
}

fn point_from_index(index: usize, arch: &Arch) -> GridPoint {
    GridPoint::new(index % arch.width, index / arch.width)
}

fn for_each_neighbor(point: GridPoint, arch: &Arch, mut visit: impl FnMut(GridPoint)) {
    if point.x > 0 {
        visit(GridPoint::new(point.x - 1, point.y));
    }
    if point.x + 1 < arch.width {
        visit(GridPoint::new(point.x + 1, point.y));
    }
    if point.y > 0 {
        visit(GridPoint::new(point.x, point.y - 1));
    }
    if point.y + 1 < arch.height {
        visit(GridPoint::new(point.x, point.y + 1));
    }
}
