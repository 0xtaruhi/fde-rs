use rustc_hash::FxHashMap as HashMap;
use smallvec::SmallVec;

use crate::{domain::NetOrigin, resource::routing::StitchedComponentDb};

use super::types::{RouteNode, RoutedPip, WireId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum RouteResource {
    Node(RouteNode),
    Sink(RouteNode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Claim {
    net_index: usize,
    origin: NetOrigin,
    from: Option<WireId>,
}

/// All claims on one physical resource, in deterministic route order.
///
/// Synthetic clock and logical-clock abstractions may share route nodes. At
/// sinks they may share only when they use the same incoming wire.
#[derive(Debug, Clone, Default)]
struct ResourceClaims(SmallVec<[Claim; 2]>);

impl ResourceClaims {
    fn shares_legally(lhs: Claim, rhs: Claim) -> bool {
        let has_synthetic =
            lhs.origin == NetOrigin::SyntheticGclk || rhs.origin == NetOrigin::SyntheticGclk;
        has_synthetic && (lhs.from.is_none() || lhs.from == rhs.from)
    }

    fn insert(&mut self, claim: Claim) -> bool {
        if self
            .0
            .iter()
            .any(|existing| existing.net_index == claim.net_index)
        {
            return false;
        }
        self.0.push(claim);
        true
    }

    fn remove(&mut self, net_index: usize) {
        self.0.retain(|claim| claim.net_index != net_index);
    }

    fn foreign_count(&self, claim: Claim) -> usize {
        self.0
            .iter()
            .filter(|existing| {
                existing.net_index != claim.net_index && !Self::shares_legally(**existing, claim)
            })
            .count()
    }

    fn overuse(&self) -> usize {
        let largest_legal_group = self
            .0
            .iter()
            .map(|candidate| {
                let mut same_source = self.0.iter().filter(|claim| claim.from == candidate.from);
                let synthetic = same_source
                    .clone()
                    .filter(|claim| claim.origin == NetOrigin::SyntheticGclk)
                    .count();
                synthetic
                    + usize::from(same_source.any(|claim| claim.origin != NetOrigin::SyntheticGclk))
            })
            .max()
            .unwrap_or(0);
        self.0.len() - largest_legal_group
    }
}

/// Bidirectional claim index used for incremental rip-up-and-reroute.
///
/// `by_resource` answers which nets must be rerouted for a conflict;
/// `by_net` makes removing precisely one net proportional to its own route;
pub(super) struct ClaimIndex {
    by_resource: HashMap<RouteResource, ResourceClaims>,
    by_net: Vec<Vec<RouteResource>>,
}

impl ClaimIndex {
    pub(super) fn new(net_count: usize) -> Self {
        Self {
            by_resource: HashMap::default(),
            by_net: vec![Vec::new(); net_count],
        }
    }

    fn claim(&mut self, resource: RouteResource, claim: Claim) {
        if self.by_resource.entry(resource).or_default().insert(claim) {
            self.by_net[claim.net_index].push(resource);
        }
    }

    pub(super) fn rip_up(&mut self, net_index: usize) {
        for resource in std::mem::take(&mut self.by_net[net_index]) {
            let remove_resource = if let Some(claims) = self.by_resource.get_mut(&resource) {
                claims.remove(net_index);
                claims.0.is_empty()
            } else {
                false
            };
            if remove_resource {
                self.by_resource.remove(&resource);
            }
        }
    }

    pub(super) fn clear(&mut self) {
        self.by_resource.clear();
        self.by_net.iter_mut().for_each(Vec::clear);
    }

    pub(super) fn contested_resources(&self) -> impl Iterator<Item = RouteResource> + '_ {
        self.by_resource
            .iter()
            .filter_map(|(resource, claims)| (claims.overuse() > 0).then_some(*resource))
    }

    pub(super) fn claimant_nets(
        &self,
        resource: RouteResource,
    ) -> impl Iterator<Item = usize> + '_ {
        self.by_resource
            .get(&resource)
            .into_iter()
            .flat_map(|claims| claims.0.iter().map(|claim| claim.net_index))
    }

    pub(super) fn overuse_count(&self) -> usize {
        self.by_resource.values().map(ResourceClaims::overuse).sum()
    }

    fn penalty(
        &self,
        resource: RouteResource,
        claim: Claim,
        history: usize,
        present_factor: usize,
    ) -> usize {
        let foreign = self
            .by_resource
            .get(&resource)
            .map_or(0, |claims| claims.foreign_count(claim));
        history + present_factor.saturating_mul(foreign)
    }

    fn blocked(&self, resource: RouteResource, claim: Claim) -> bool {
        self.by_resource
            .get(&resource)
            .is_some_and(|claims| claims.foreign_count(claim) > 0)
    }
}

pub(super) type HistoryTable = HashMap<RouteResource, usize>;

pub(super) fn bump_history(history: &mut HistoryTable, resource: RouteResource, increment: usize) {
    *history.entry(resource).or_insert(0) += increment;
}

pub(super) fn reserve_route_path(
    stitched_components: &StitchedComponentDb,
    claims: &mut ClaimIndex,
    net_index: usize,
    origin: NetOrigin,
    path_nodes: &[RouteNode],
    path_pips: &[RoutedPip],
) {
    for pip in path_pips {
        claims.claim(
            RouteResource::Sink(RouteNode::new(pip.x, pip.y, pip.to)),
            Claim {
                net_index,
                origin,
                from: Some(pip.from),
            },
        );
    }
    for node in path_nodes {
        claims.claim(
            RouteResource::Node(stitched_components.occupancy_key(node)),
            Claim {
                net_index,
                origin,
                from: None,
            },
        );
    }
}

/// Everything a search needs to price resource contention for one net.
pub(super) struct NegotiationContext<'a> {
    pub(super) claims: &'a ClaimIndex,
    pub(super) history: &'a HistoryTable,
    pub(super) present_factor: usize,
    pub(super) net_index: usize,
    pub(super) net_origin: NetOrigin,
    pub(super) hard_block: bool,
}

impl NegotiationContext<'_> {
    fn claim(&self, from: Option<WireId>) -> Claim {
        Claim {
            net_index: self.net_index,
            origin: self.net_origin,
            from,
        }
    }

    pub(super) fn penalty(&self, resource: RouteResource, from: Option<WireId>) -> usize {
        self.claims.penalty(
            resource,
            self.claim(from),
            self.history.get(&resource).copied().unwrap_or(0),
            self.present_factor,
        )
    }

    pub(super) fn blocked(&self, resource: RouteResource, from: Option<WireId>) -> bool {
        self.claims.blocked(resource, self.claim(from))
    }
}
