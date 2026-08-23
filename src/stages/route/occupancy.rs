use rustc_hash::FxHashMap as HashMap;

use crate::domain::NetOrigin;
use crate::resource::routing::StitchedComponentDb;

use super::types::{RouteNode, RoutedPip, WireId};
type RouteWireKey = RouteNode;

/// Per-node claim bookkeeping for negotiated congestion.
///
/// `owner` is the net that claimed the resource first within the current
/// negotiation pass; `others` counts claims by *different* nets afterwards.
/// Same-net revisits (shared multi-sink trees) never bump `others`.
///
/// Synthetic gclk nets are exempt from contention counting entirely: they
/// share physical pad/clock wires with user nets by construction (both are
/// abstractions of the same silicon path), so no amount of negotiation can
/// separate them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ResourceClaims {
    pub(super) owner: usize,
    pub(super) owner_origin: NetOrigin,
    pub(super) others: usize,
}

impl ResourceClaims {
    pub(super) fn new(net_index: usize, origin: NetOrigin) -> Self {
        Self {
            owner: net_index,
            owner_origin: origin,
            others: 0,
        }
    }

    fn shares_legally(&self, origin: NetOrigin) -> bool {
        self.owner_origin == NetOrigin::SyntheticGclk || origin == NetOrigin::SyntheticGclk
    }

    pub(super) fn claim(&mut self, net_index: usize, origin: NetOrigin) {
        if self.owner != net_index && !self.shares_legally(origin) {
            self.others += 1;
        }
    }

    /// Congestion penalty in cost units for a net about to enter this
    /// resource. Same-net resources only pay accumulated history; foreign
    /// claims additionally pay the present-sharing factor scaled by how
    /// contested the resource already is.
    pub(super) fn congestion_penalty(
        &self,
        net_index: usize,
        origin: NetOrigin,
        history: usize,
        present_factor: usize,
    ) -> usize {
        if self.owner == net_index || self.shares_legally(origin) {
            return history;
        }
        history + present_factor.saturating_mul(self.others + 1)
    }
}

/// Sink-side claims keep the legacy gclk-sharing exception: a synthetic
/// gclk net and the user clock net legitimately drive the same sink arc
/// when they arrive over the same source wire, and forcing them apart is
/// unroutable (they have no alternative path onto the clock spine).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SinkClaims {
    pub(super) owner_net: usize,
    pub(super) owner_origin: NetOrigin,
    pub(super) owner_from: WireId,
    pub(super) others: usize,
}

impl SinkClaims {
    pub(super) fn new(net_index: usize, origin: NetOrigin, from: WireId) -> Self {
        Self {
            owner_net: net_index,
            owner_origin: origin,
            owner_from: from,
            others: 0,
        }
    }

    fn shares_legally(&self, origin: NetOrigin, from: WireId) -> bool {
        self.owner_origin == NetOrigin::SyntheticGclk
            || origin == NetOrigin::SyntheticGclk && self.owner_from == from
    }

    pub(super) fn claim(&mut self, net_index: usize, origin: NetOrigin, from: WireId) {
        if self.owner_net == net_index || self.shares_legally(origin, from) {
            return;
        }
        self.others += 1;
    }

    pub(super) fn congestion_penalty(
        &self,
        net_index: usize,
        origin: NetOrigin,
        from: WireId,
        history: usize,
        present_factor: usize,
    ) -> usize {
        if self.owner_net == net_index || self.shares_legally(origin, from) {
            return history;
        }
        history + present_factor.saturating_mul(self.others + 1)
    }
}

/// Historical congestion memory for one resource: grows every negotiation
/// pass in which the resource ended up contested, making previously
/// over-subscribed wires progressively less attractive.
pub(super) type HistoryTable = HashMap<RouteWireKey, usize>;

pub(super) fn history_of(history: &HistoryTable, key: &RouteWireKey) -> usize {
    history.get(key).copied().unwrap_or(0)
}

pub(super) fn bump_history(history: &mut HistoryTable, key: &RouteWireKey, increment: usize) {
    *history.entry(*key).or_insert(0) += increment;
}

pub(super) fn reserve_route_sinks(
    occupied_route_sinks: &mut HashMap<RouteWireKey, SinkClaims>,
    net_index: usize,
    origin: NetOrigin,
    path: &[RoutedPip],
) {
    for pip in path {
        occupied_route_sinks
            .entry(RouteNode::new(pip.x, pip.y, pip.to))
            .and_modify(|claims| claims.claim(net_index, origin, pip.from))
            .or_insert_with(|| SinkClaims::new(net_index, origin, pip.from));
    }
}

pub(super) fn reserve_route_nodes(
    stitched_components: &StitchedComponentDb,
    occupied_route_nodes: &mut HashMap<RouteNode, ResourceClaims>,
    net_index: usize,
    origin: NetOrigin,
    path_nodes: &[RouteNode],
) {
    for &node in path_nodes {
        let key = stitched_components.occupancy_key(&node);
        occupied_route_nodes
            .entry(key)
            .and_modify(|claims| claims.claim(net_index, origin))
            .or_insert_with(|| ResourceClaims::new(net_index, origin));
    }
}

/// Count resources claimed by more than one net in the finished pass.
pub(super) fn count_contested<K, C>(claims: &HashMap<K, C>) -> impl Iterator<Item = (&K, usize)>
where
    C: Contended,
{
    claims
        .iter()
        .filter_map(|(key, c)| (c.others() > 0).then_some((key, c.others())))
}

/// Uniform access to the foreign-claim counter across claim record types.
pub(super) trait Contended {
    fn others(&self) -> usize;
}

impl Contended for ResourceClaims {
    fn others(&self) -> usize {
        self.others
    }
}

impl Contended for SinkClaims {
    fn others(&self) -> usize {
        self.others
    }
}

/// Penalty for an unclaimed resource: only its accumulated history applies.
pub(super) fn unclaimed_penalty(history: usize) -> usize {
    history
}

/// Everything a search needs to price resource contention for one net.
pub(super) struct NegotiationContext<'a> {
    pub(super) occupied_route_sinks: &'a HashMap<RouteNode, SinkClaims>,
    pub(super) occupied_route_nodes: &'a HashMap<RouteNode, ResourceClaims>,
    pub(super) node_history: &'a HistoryTable,
    pub(super) sink_history: &'a HistoryTable,
    pub(super) present_factor: usize,
    pub(super) net_index: usize,
    pub(super) net_origin: NetOrigin,
    pub(super) hard_block: bool,
}

impl NegotiationContext<'_> {
    pub(super) fn node_penalty(&self, key: &RouteNode) -> usize {
        self.occupied_route_nodes.get(key).map_or_else(
            || unclaimed_penalty(history_of(self.node_history, key)),
            |claims| {
                claims.congestion_penalty(
                    self.net_index,
                    self.net_origin,
                    history_of(self.node_history, key),
                    self.present_factor,
                )
            },
        )
    }
}
