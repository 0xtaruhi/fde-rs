use super::{DeviceCell, DeviceDesign, DeviceEndpoint, DeviceLowering, DeviceNet, DevicePort};
use crate::device::DeviceSinkGuide;
use crate::ir::{Endpoint, RouteSegment};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

struct LoweredLookups {
    ports: BTreeMap<String, DevicePort>,
    cells: BTreeMap<String, DeviceCell>,
}

impl<'a> DeviceLowering<'a> {
    pub(super) fn materialize_nets(&mut self) {
        let lookups = LoweredLookups::build(&self.device);
        self.materialize_input_nets(&lookups);
        self.materialize_logical_nets(&lookups);
        self.materialize_output_nets(&lookups);
    }

    fn materialize_input_nets(&mut self, lookups: &LoweredLookups) {
        for port in &self.design.ports {
            if !port.direction.is_input_like() {
                continue;
            }
            let Some(driver) = lookups.port(&port.name) else {
                continue;
            };
            let Some(io_cell) = self
                .port_to_io
                .get(&port.name)
                .and_then(|name| lookups.cell(name))
            else {
                continue;
            };
            self.device.nets.push(DeviceNet {
                name: format!("pad::{}", port.name),
                driver: Some(port_endpoint(driver)),
                sinks: vec![cell_endpoint(io_cell, "PAD")],
                origin: "synthetic-pad-input".to_string(),
                route_pips: Vec::new(),
                guide_tiles: Vec::new(),
                sink_guides: Vec::new(),
            });
            if let Some(gclk_name) = self.port_to_gclk.get(&port.name)
                && let Some(gclk_cell) = lookups.cell(gclk_name)
            {
                self.device.nets.push(DeviceNet {
                    name: format!("gclk::{}", port.name),
                    driver: Some(cell_endpoint(io_cell, "GCLKOUT")),
                    sinks: vec![cell_endpoint(gclk_cell, "IN")],
                    origin: "synthetic-gclk".to_string(),
                    route_pips: Vec::new(),
                    guide_tiles: Vec::new(),
                    sink_guides: Vec::new(),
                });
            }
        }
    }

    fn materialize_logical_nets(&mut self, lookups: &LoweredLookups) {
        for net in &self.design.nets {
            let driver = net
                .driver
                .as_ref()
                .and_then(|endpoint| self.lowered_endpoint(endpoint, lookups, true));
            let sinks = net
                .sinks
                .iter()
                .filter_map(|endpoint| self.lowered_endpoint(endpoint, lookups, false))
                .collect::<Vec<_>>();
            let sink_guides = sink_guides(driver.as_ref(), &sinks, &net.route);
            self.device.nets.push(DeviceNet {
                name: net.name.clone(),
                driver,
                sinks,
                origin: "logical-net".to_string(),
                route_pips: net.route_pips.clone(),
                guide_tiles: guide_tiles(&net.route),
                sink_guides,
            });
        }
    }

    fn materialize_output_nets(&mut self, lookups: &LoweredLookups) {
        for port in &self.design.ports {
            if !port.direction.is_output_like() {
                continue;
            }
            let Some(port_binding) = lookups.port(&port.name) else {
                continue;
            };
            let Some(io_cell) = self
                .port_to_io
                .get(&port.name)
                .and_then(|name| lookups.cell(name))
            else {
                continue;
            };
            self.device.nets.push(DeviceNet {
                name: format!("pad::{}", port.name),
                driver: Some(cell_endpoint(io_cell, "PAD")),
                sinks: vec![port_endpoint(port_binding)],
                origin: "synthetic-pad-output".to_string(),
                route_pips: Vec::new(),
                guide_tiles: Vec::new(),
                sink_guides: Vec::new(),
            });
        }
    }

    fn lowered_endpoint(
        &self,
        endpoint: &Endpoint,
        lookups: &LoweredLookups,
        is_driver: bool,
    ) -> Option<DeviceEndpoint> {
        match endpoint.kind.as_str() {
            "cell" => lookups
                .cell(&endpoint.name)
                .map(|cell| cell_endpoint(cell, &endpoint.pin)),
            "port" => {
                let port = lookups.port(&endpoint.name)?;
                if is_driver && port.direction.is_input_like() {
                    if let Some(name) = self.port_to_gclk.get(&endpoint.name) {
                        return lookups.cell(name).map(|cell| cell_endpoint(cell, "OUT"));
                    }
                    if let Some(name) = self.port_to_io.get(&endpoint.name) {
                        return lookups.cell(name).map(|cell| cell_endpoint(cell, "IN"));
                    }
                }
                if !is_driver
                    && port.direction.is_output_like()
                    && let Some(name) = self.port_to_io.get(&endpoint.name)
                {
                    return lookups.cell(name).map(|cell| cell_endpoint(cell, "OUT"));
                }
                Some(port_endpoint(port))
            }
            _ => None,
        }
    }
}

impl LoweredLookups {
    fn build(device: &DeviceDesign) -> Self {
        Self {
            ports: device
                .ports
                .iter()
                .cloned()
                .map(|port| (port.port_name.clone(), port))
                .collect::<BTreeMap<_, _>>(),
            cells: device
                .cells
                .iter()
                .cloned()
                .map(|cell| (cell.cell_name.clone(), cell))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    fn port(&self, name: &str) -> Option<&DevicePort> {
        self.ports.get(name)
    }

    fn cell(&self, name: &str) -> Option<&DeviceCell> {
        self.cells.get(name)
    }
}

fn port_endpoint(port: &DevicePort) -> DeviceEndpoint {
    DeviceEndpoint {
        kind: "port".to_string(),
        name: port.port_name.clone(),
        pin: port.pin_name.clone(),
        x: port.x,
        y: port.y,
        z: port.z,
    }
}

fn cell_endpoint(cell: &DeviceCell, pin: &str) -> DeviceEndpoint {
    DeviceEndpoint {
        kind: "cell".to_string(),
        name: cell.cell_name.clone(),
        pin: pin.to_string(),
        x: cell.x,
        y: cell.y,
        z: cell.z,
    }
}

fn guide_tiles(route: &[RouteSegment]) -> Vec<(usize, usize)> {
    let mut tiles = Vec::new();
    for segment in route {
        append_segment_tiles(&mut tiles, segment);
    }
    tiles.dedup();
    tiles
}

fn sink_guides(
    driver: Option<&DeviceEndpoint>,
    sinks: &[DeviceEndpoint],
    route: &[RouteSegment],
) -> Vec<DeviceSinkGuide> {
    let Some(driver) = driver else {
        return Vec::new();
    };

    let adjacency = route_adjacency(route);
    let source = (driver.x, driver.y);
    sinks
        .iter()
        .filter_map(|sink| {
            trace_route_path(source, (sink.x, sink.y), &adjacency).map(|tiles| DeviceSinkGuide {
                sink: sink.clone(),
                tiles,
            })
        })
        .collect()
}

fn route_adjacency(route: &[RouteSegment]) -> BTreeMap<(usize, usize), BTreeSet<(usize, usize)>> {
    let mut adjacency = BTreeMap::<(usize, usize), BTreeSet<(usize, usize)>>::new();
    for segment in route {
        let mut segment_tiles = Vec::new();
        append_segment_tiles(&mut segment_tiles, segment);
        for tile in &segment_tiles {
            adjacency.entry(*tile).or_default();
        }
        for window in segment_tiles.windows(2) {
            if let [from, to] = window {
                adjacency.entry(*from).or_default().insert(*to);
                adjacency.entry(*to).or_default().insert(*from);
            }
        }
    }
    adjacency
}

fn trace_route_path(
    source: (usize, usize),
    target: (usize, usize),
    adjacency: &BTreeMap<(usize, usize), BTreeSet<(usize, usize)>>,
) -> Option<Vec<(usize, usize)>> {
    if source == target {
        return Some(vec![source]);
    }
    if !adjacency.contains_key(&source) || !adjacency.contains_key(&target) {
        return None;
    }

    let mut queue = VecDeque::from([source]);
    let mut seen = BTreeSet::from([source]);
    let mut parent = BTreeMap::<(usize, usize), (usize, usize)>::new();

    while let Some(current) = queue.pop_front() {
        let Some(neighbors) = adjacency.get(&current) else {
            continue;
        };
        for neighbor in neighbors {
            if !seen.insert(*neighbor) {
                continue;
            }
            parent.insert(*neighbor, current);
            if *neighbor == target {
                return Some(reconstruct_tile_path(source, target, &parent));
            }
            queue.push_back(*neighbor);
        }
    }

    None
}

fn reconstruct_tile_path(
    source: (usize, usize),
    target: (usize, usize),
    parent: &BTreeMap<(usize, usize), (usize, usize)>,
) -> Vec<(usize, usize)> {
    let mut path = vec![target];
    let mut current = target;
    while current != source {
        let Some(previous) = parent.get(&current).copied() else {
            break;
        };
        current = previous;
        path.push(current);
    }
    path.reverse();
    path
}

fn append_segment_tiles(tiles: &mut Vec<(usize, usize)>, segment: &RouteSegment) {
    let dx = match segment.x1.cmp(&segment.x0) {
        std::cmp::Ordering::Less => -1isize,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    };
    let dy = match segment.y1.cmp(&segment.y0) {
        std::cmp::Ordering::Less => -1isize,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    };
    let steps = segment
        .x0
        .abs_diff(segment.x1)
        .max(segment.y0.abs_diff(segment.y1));
    let mut x = segment.x0 as isize;
    let mut y = segment.y0 as isize;

    for _ in 0..=steps {
        let point = (x as usize, y as usize);
        if tiles.last().copied() != Some(point) {
            tiles.push(point);
        }
        x += dx;
        y += dy;
    }
}

#[cfg(test)]
mod tests {
    use super::sink_guides;
    use crate::{device::DeviceEndpoint, ir::RouteSegment};

    fn endpoint(name: &str, pin: &str, x: usize, y: usize) -> DeviceEndpoint {
        DeviceEndpoint {
            kind: "cell".to_string(),
            name: name.to_string(),
            pin: pin.to_string(),
            x,
            y,
            z: 0,
        }
    }

    #[test]
    fn sink_guides_follow_branch_specific_paths() {
        let driver = endpoint("src", "O", 0, 0);
        let sinks = vec![endpoint("dst_a", "I0", 0, 2), endpoint("dst_b", "I0", 1, 1)];
        let route = vec![
            RouteSegment {
                x0: 0,
                y0: 0,
                x1: 0,
                y1: 1,
            },
            RouteSegment {
                x0: 0,
                y0: 1,
                x1: 0,
                y1: 2,
            },
            RouteSegment {
                x0: 0,
                y0: 1,
                x1: 1,
                y1: 1,
            },
        ];

        let guides = sink_guides(Some(&driver), &sinks, &route);
        assert_eq!(guides.len(), 2);
        assert_eq!(guides[0].sink, sinks[0]);
        assert_eq!(guides[0].tiles, vec![(0, 0), (0, 1), (0, 2)]);
        assert_eq!(guides[1].sink, sinks[1]);
        assert_eq!(guides[1].tiles, vec![(0, 0), (0, 1), (1, 1)]);
    }
}
