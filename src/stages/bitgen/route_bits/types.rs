use arrayvec::ArrayString;
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::HashMap, fmt::Write};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceRouteImage {
    #[serde(default)]
    pub pips: Vec<DeviceRoutePip>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceRoutePip {
    pub net_name: String,
    pub tile_name: String,
    pub tile_type: String,
    pub site_name: String,
    pub site_type: String,
    pub x: usize,
    pub y: usize,
    pub from_net: String,
    pub to_net: String,
    #[serde(default)]
    pub bits: Vec<RouteBit>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RouteBit {
    pub basic_cell: String,
    pub sram_name: String,
    pub value: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct WireId(u32);

impl WireId {
    fn new(index: usize) -> Self {
        Self(index as u32)
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WireInterner {
    ids_by_name: HashMap<String, WireId>,
    names: Vec<String>,
}

impl WireInterner {
    pub(crate) fn intern(&mut self, name: &str) -> WireId {
        if let Some(id) = self.id(name) {
            return id;
        }
        self.intern_owned(name.to_string())
    }

    pub(crate) fn intern_owned(&mut self, name: String) -> WireId {
        if let Some(id) = self.id(&name) {
            return id;
        }
        let id = WireId::new(self.names.len());
        self.ids_by_name.insert(name.clone(), id);
        self.names.push(name);
        id
    }

    pub(crate) fn id(&self, name: &str) -> Option<WireId> {
        self.ids_by_name.get(name).copied()
    }

    pub(crate) fn intern_indexed(&mut self, prefix: &str, index: usize) -> WireId {
        self.intern_composite_indexed(prefix, "", index, "")
    }

    pub(crate) fn intern_composite_indexed(
        &mut self,
        first: &str,
        second: &str,
        index: usize,
        third: &str,
    ) -> WireId {
        let mut stack = ArrayString::<48>::new();
        if stack.try_push_str(first).is_ok()
            && stack.try_push_str(second).is_ok()
            && write!(&mut stack, "{index}").is_ok()
            && stack.try_push_str(third).is_ok()
        {
            if let Some(id) = self.id(stack.as_str()) {
                return id;
            }
            return self.intern_owned(stack.as_str().to_owned());
        }

        let mut heap =
            String::with_capacity(first.len() + second.len() + third.len() + usize::BITS as usize);
        heap.push_str(first);
        heap.push_str(second);
        let _ = write!(&mut heap, "{index}");
        heap.push_str(third);
        self.intern_owned(heap)
    }

    pub(crate) fn resolve(&self, id: WireId) -> &str {
        self.names
            .get(id.index())
            .map(String::as_str)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SiteRouteGraph {
    pub(crate) arcs: Vec<SiteRouteArc>,
    pub(crate) adjacency: HashMap<WireId, Vec<usize>>,
    pub(crate) default_bits: Vec<RouteBit>,
}

pub(crate) type SiteRouteGraphs = HashMap<String, SiteRouteGraph>;

#[derive(Debug, Clone)]
pub(crate) struct SiteRouteArc {
    pub(crate) from: WireId,
    pub(crate) to: WireId,
    pub(crate) basic_cell: String,
    pub(crate) bits: Vec<RouteBit>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RoutedPip {
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) from: WireId,
    pub(crate) to: WireId,
    pub(crate) local_arc: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GlobalState {
    pub(crate) cost: usize,
    pub(crate) priority: usize,
    pub(crate) node: RouteNode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RouteNode {
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) wire: WireId,
}

impl RouteNode {
    pub(crate) fn new(x: usize, y: usize, wire: WireId) -> Self {
        Self { x, y, wire }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ParentStep {
    pub(crate) previous: RouteNode,
    pub(crate) local_arc: Option<usize>,
}

impl Eq for GlobalState {}

impl PartialEq for GlobalState {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.cost == other.cost && self.node == other.node
    }
}

impl Ord for GlobalState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| other.cost.cmp(&self.cost))
            .then_with(|| self.node.wire.cmp(&other.node.wire))
            .then_with(|| self.node.x.cmp(&other.node.x))
            .then_with(|| self.node.y.cmp(&other.node.y))
    }
}

impl PartialOrd for GlobalState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
