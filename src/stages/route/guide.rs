use smallvec::SmallVec;
use std::collections::VecDeque;

use super::types::RouteNode;
use crate::resource::Arch;

#[derive(Debug, Clone, Copy)]
pub(super) enum GuideRouteMode {
    Ordered,
    Strict,
    Relaxed,
    Fallback,
    Unguided,
    DedicatedClock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct GuidedRouteNode {
    pub(super) node: RouteNode,
    pub(super) guide_index: usize,
}

#[derive(Debug, Clone)]
pub(super) struct OrderedGuide {
    tiles: Vec<(usize, usize)>,
}

impl OrderedGuide {
    pub(super) fn new(tiles: &[(usize, usize)]) -> Self {
        let mut ordered = Vec::with_capacity(tiles.len());
        for &tile in tiles {
            if ordered.last().copied() != Some(tile) {
                ordered.push(tile);
            }
        }
        Self { tiles: ordered }
    }

    pub(super) fn is_active(&self) -> bool {
        !self.tiles.is_empty()
    }

    pub(super) fn len(&self) -> usize {
        self.tiles.len()
    }

    pub(super) fn last_index(&self) -> usize {
        self.tiles.len().saturating_sub(1)
    }

    pub(super) fn last_tile(&self) -> Option<(usize, usize)> {
        self.tiles.last().copied()
    }

    pub(super) fn indices_for_tile(&self, tile: (usize, usize)) -> SmallVec<[usize; 4]> {
        self.tiles
            .iter()
            .enumerate()
            .filter_map(|(index, &candidate)| (candidate == tile).then_some(index))
            .collect()
    }

    pub(super) fn remaining_steps(&self, index: usize) -> usize {
        self.last_index().saturating_sub(index)
    }

    pub(super) fn advance(
        &self,
        current_index: usize,
        current_tile: (usize, usize),
        next_tile: (usize, usize),
    ) -> Option<usize> {
        if !self.is_active() || self.tiles.get(current_index).copied()? != current_tile {
            return None;
        }
        if next_tile == current_tile {
            return Some(current_index);
        }
        for next_index in (current_index + 1)..self.tiles.len() {
            if self.tiles[next_index] != next_tile {
                continue;
            }
            if guide_run_is_linear(&self.tiles[current_index..=next_index]) {
                return Some(next_index);
            }
        }
        None
    }
}

pub(super) struct GuideDistances {
    width: usize,
    height: usize,
    field: Option<Vec<usize>>,
}

impl GuideDistances {
    pub(super) fn new(arch: &Arch, guide_tiles: &[(usize, usize)]) -> Self {
        if guide_tiles.is_empty() || arch.width == 0 || arch.height == 0 {
            return Self {
                width: arch.width,
                height: arch.height,
                field: None,
            };
        }

        let size = arch.width.saturating_mul(arch.height);
        let mut field = vec![usize::MAX; size];
        let mut queue = VecDeque::new();
        for &(x, y) in guide_tiles {
            if x >= arch.width || y >= arch.height || arch.tile_at(x, y).is_none() {
                continue;
            }
            let index = y * arch.width + x;
            if field[index] == 0 {
                continue;
            }
            field[index] = 0;
            queue.push_back((x, y));
        }

        while let Some((x, y)) = queue.pop_front() {
            let index = y * arch.width + x;
            let base = field[index];
            for (nx, ny) in tile_neighbors(arch, x, y) {
                let next_index = ny * arch.width + nx;
                if base + 1 < field[next_index] {
                    field[next_index] = base + 1;
                    queue.push_back((nx, ny));
                }
            }
        }

        Self {
            width: arch.width,
            height: arch.height,
            field: Some(field),
        }
    }

    pub(super) fn distance(&self, x: usize, y: usize) -> usize {
        self.field
            .as_ref()
            .and_then(|field| {
                if x >= self.width || y >= self.height {
                    None
                } else {
                    field.get(y * self.width + x).copied()
                }
            })
            .unwrap_or(0)
    }

    pub(super) fn is_active(&self) -> bool {
        self.field.is_some()
    }
}

pub(super) fn guide_penalty(
    current: &RouteNode,
    neighbor: &RouteNode,
    guide_distances: &GuideDistances,
) -> usize {
    if !guide_distances.is_active() || (current.x == neighbor.x && current.y == neighbor.y) {
        return 0;
    }

    let current_distance = guide_distances.distance(current.x, current.y);
    let next_distance = guide_distances.distance(neighbor.x, neighbor.y);
    let next_distance = if next_distance == usize::MAX {
        32
    } else {
        next_distance.min(32)
    };
    let drift = next_distance.saturating_sub(current_distance.saturating_add(1));
    next_distance.saturating_mul(4) + drift.saturating_mul(6)
}

fn guide_run_is_linear(run: &[(usize, usize)]) -> bool {
    if run.len() < 2 {
        return true;
    }
    let Some(direction) = guide_step(run[0], run[1]) else {
        return false;
    };
    run.windows(2)
        .all(|window| matches!(window, [from, to] if guide_step(*from, *to) == Some(direction)))
}

fn guide_step(from: (usize, usize), to: (usize, usize)) -> Option<(isize, isize)> {
    let dx = to.0 as isize - from.0 as isize;
    let dy = to.1 as isize - from.1 as isize;
    match (dx, dy) {
        (-1 | 1, 0) | (0, -1 | 1) => Some((dx.signum(), dy.signum())),
        _ => None,
    }
}

fn tile_neighbors(arch: &Arch, x: usize, y: usize) -> SmallVec<[(usize, usize); 4]> {
    let mut neighbors = SmallVec::new();
    for (nx, ny) in [
        (x.wrapping_sub(1), y),
        (x + 1, y),
        (x, y.wrapping_sub(1)),
        (x, y + 1),
    ] {
        if nx < arch.width && ny < arch.height && arch.tile_at(nx, ny).is_some() {
            neighbors.push((nx, ny));
        }
    }
    neighbors
}
