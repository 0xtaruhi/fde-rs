use crate::place::{PlaceMode, PlaceOptions, manhattan};
use anyhow::{Result, anyhow, bail};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::{BTreeMap, BTreeSet};

use super::{
    cost::{PlacementCandidate, PlacementEvaluator, PlacementMetrics, evaluate},
    graph::{ClusterGraph, build_cluster_graph, cluster_incident_criticality, weighted_centroid},
    model::PlacementModel,
};

const INCREMENTAL_EVALUATOR_NET_THRESHOLD: usize = 128;

#[derive(Debug, Clone)]
pub(crate) struct PlacementSolution {
    pub(crate) placements: BTreeMap<String, (usize, usize)>,
    pub(crate) metrics: PlacementMetrics,
}

struct SolveContext<'a> {
    design: &'a crate::ir::Design,
    options: &'a PlaceOptions,
    graph: &'a ClusterGraph,
    model: &'a PlacementModel,
    criticality: &'a BTreeMap<String, f64>,
    sites: &'a [(usize, usize)],
    site_set: &'a BTreeSet<(usize, usize)>,
    movable: &'a [String],
    movable_set: &'a BTreeSet<String>,
}

type FullPlacementCandidate = (BTreeMap<String, (usize, usize)>, PlacementMetrics);

pub(crate) fn solve(
    design: &crate::ir::Design,
    options: &PlaceOptions,
) -> Result<PlacementSolution> {
    solve_internal(design, options, None)
}

#[cfg(test)]
pub(crate) fn solve_for_test(
    design: &crate::ir::Design,
    options: &PlaceOptions,
    use_incremental: bool,
) -> Result<PlacementSolution> {
    solve_internal(design, options, Some(use_incremental))
}

fn solve_internal(
    design: &crate::ir::Design,
    options: &PlaceOptions,
    incremental_override: Option<bool>,
) -> Result<PlacementSolution> {
    let sites = options.arch.logic_sites();
    let site_set = sites.iter().copied().collect::<BTreeSet<_>>();
    let graph = build_cluster_graph(design);
    let model = PlacementModel::from_design(design);
    let criticality = cluster_incident_criticality(design);
    let movable = design
        .clusters
        .iter()
        .filter(|cluster| !cluster.fixed)
        .map(|cluster| cluster.name.clone())
        .collect::<Vec<_>>();

    if movable.len() <= 1 {
        let current = initial_placement(design, &graph, &model, &criticality, &sites, &site_set)?;
        let metrics = evaluate(
            &model,
            &graph,
            &current,
            &options.arch,
            options.delay.as_ref(),
            options.mode,
        );
        return Ok(PlacementSolution {
            placements: current,
            metrics,
        });
    }

    let movable_set = movable.iter().cloned().collect::<BTreeSet<_>>();
    let context = SolveContext {
        design,
        options,
        graph: &graph,
        model: &model,
        criticality: &criticality,
        sites: &sites,
        site_set: &site_set,
        movable: &movable,
        movable_set: &movable_set,
    };

    let use_incremental =
        incremental_override.unwrap_or(model.nets.len() >= INCREMENTAL_EVALUATOR_NET_THRESHOLD);
    if use_incremental {
        solve_incremental(&context)
    } else {
        solve_full(&context)
    }
}

fn solve_incremental(context: &SolveContext<'_>) -> Result<PlacementSolution> {
    let mut rng = ChaCha8Rng::seed_from_u64(context.options.seed);
    let current = initial_placement(
        context.design,
        context.graph,
        context.model,
        context.criticality,
        context.sites,
        context.site_set,
    )?;
    let mut evaluator = PlacementEvaluator::new(
        context.model,
        context.graph,
        current,
        &context.options.arch,
        context.options.delay.as_ref(),
        context.options.mode,
    );
    let mut current_occupancy = occupancy_map(evaluator.placements());
    let mut current_metrics = evaluator.metrics().clone();
    let mut best = evaluator.placements().clone();
    let mut best_metrics = evaluator.metrics().clone();
    let focus_weights = focus_weights(context);

    let iterations = 700 + context.movable.len() * 50;
    let mut temperature = (current_metrics.total / context.movable.len().max(1) as f64).max(0.5);
    let mut stall = 0usize;

    for step in 0..iterations {
        let focus = choose_focus(&focus_weights, &mut rng)
            .ok_or_else(|| anyhow!("missing movable cluster during placement"))?;
        let candidates = candidate_targets(
            focus,
            context.model,
            context.graph,
            evaluator.placements(),
            context.sites,
            context.site_set,
            &mut rng,
        );

        let mut best_trial: Option<PlacementCandidate> = None;
        for target in candidates {
            let Some(changes) = plan_target_updates(
                evaluator.placements(),
                &current_occupancy,
                context.movable_set,
                focus,
                target,
            ) else {
                continue;
            };
            let trial = evaluator.evaluate_candidate(&changes);
            let metrics = trial.metrics();
            if best_trial
                .as_ref()
                .is_none_or(|best_candidate| metrics.total < best_candidate.metrics().total)
            {
                best_trial = Some(trial);
            }
        }

        let Some(trial) = best_trial else {
            continue;
        };
        let trial_metrics = trial.metrics().clone();
        let improved = trial_metrics.total + 1e-9 < current_metrics.total;
        let accept = if improved {
            true
        } else {
            let delta = trial_metrics.total - current_metrics.total;
            let threshold = (-delta / temperature.max(0.01)).exp().clamp(0.0, 1.0);
            rng.random::<f64>() < threshold
        };

        if accept {
            evaluator.apply_candidate(trial);
            current_occupancy = occupancy_map(evaluator.placements());
            current_metrics = trial_metrics;
            if current_metrics.total + 1e-9 < best_metrics.total {
                best = evaluator.placements().clone();
                best_metrics = current_metrics.clone();
                stall = 0;
            } else {
                stall += 1;
            }
        } else {
            stall += 1;
        }

        if stall > context.movable.len() * 3 {
            if let Some(swapped) =
                random_swap_updates(evaluator.placements(), context.movable, &mut rng)
            {
                let swap_candidate = evaluator.evaluate_candidate(&swapped);
                let swap_metrics = swap_candidate.metrics().clone();
                if swap_metrics.total < current_metrics.total {
                    evaluator.apply_candidate(swap_candidate);
                    current_occupancy = occupancy_map(evaluator.placements());
                    current_metrics = swap_metrics;
                    if current_metrics.total < best_metrics.total {
                        best = evaluator.placements().clone();
                        best_metrics = current_metrics.clone();
                    }
                }
            }
            stall = 0;
        }

        temperature *= if step % 40 == 0 { 0.985 } else { 0.9985 };
        temperature = temperature.max(0.02);
    }

    Ok(PlacementSolution {
        placements: best,
        metrics: best_metrics,
    })
}

fn solve_full(context: &SolveContext<'_>) -> Result<PlacementSolution> {
    let mut rng = ChaCha8Rng::seed_from_u64(context.options.seed);
    let mut current = initial_placement(
        context.design,
        context.graph,
        context.model,
        context.criticality,
        context.sites,
        context.site_set,
    )?;
    let mut current_occupancy = occupancy_map(&current);
    let mut current_metrics = evaluate(
        context.model,
        context.graph,
        &current,
        &context.options.arch,
        context.options.delay.as_ref(),
        context.options.mode,
    );
    let mut best = current.clone();
    let mut best_metrics = current_metrics.clone();
    let focus_weights = focus_weights(context);

    let iterations = 700 + context.movable.len() * 50;
    let mut temperature = (current_metrics.total / context.movable.len().max(1) as f64).max(0.5);
    let mut stall = 0usize;

    for step in 0..iterations {
        let focus = choose_focus(&focus_weights, &mut rng)
            .ok_or_else(|| anyhow!("missing movable cluster during placement"))?;
        let candidates = candidate_targets(
            focus,
            context.model,
            context.graph,
            &current,
            context.sites,
            context.site_set,
            &mut rng,
        );

        let mut best_trial: Option<FullPlacementCandidate> = None;
        for target in candidates {
            let Some(changes) = plan_target_updates(
                &current,
                &current_occupancy,
                context.movable_set,
                focus,
                target,
            ) else {
                continue;
            };
            let trial = apply_updates(&current, &changes);
            let metrics = evaluate(
                context.model,
                context.graph,
                &trial,
                &context.options.arch,
                context.options.delay.as_ref(),
                context.options.mode,
            );
            if best_trial
                .as_ref()
                .is_none_or(|(_, best_metrics)| metrics.total < best_metrics.total)
            {
                best_trial = Some((trial, metrics));
            }
        }

        let Some((trial, trial_metrics)) = best_trial else {
            continue;
        };
        let improved = trial_metrics.total + 1e-9 < current_metrics.total;
        let accept = if improved {
            true
        } else {
            let delta = trial_metrics.total - current_metrics.total;
            let threshold = (-delta / temperature.max(0.01)).exp().clamp(0.0, 1.0);
            rng.random::<f64>() < threshold
        };

        if accept {
            current = trial;
            current_occupancy = occupancy_map(&current);
            current_metrics = trial_metrics;
            if current_metrics.total + 1e-9 < best_metrics.total {
                best = current.clone();
                best_metrics = current_metrics.clone();
                stall = 0;
            } else {
                stall += 1;
            }
        } else {
            stall += 1;
        }

        if stall > context.movable.len() * 3 {
            if let Some(swapped) = random_swap_updates(&current, context.movable, &mut rng) {
                let trial = apply_updates(&current, &swapped);
                let swap_metrics = evaluate(
                    context.model,
                    context.graph,
                    &trial,
                    &context.options.arch,
                    context.options.delay.as_ref(),
                    context.options.mode,
                );
                if swap_metrics.total < current_metrics.total {
                    current = trial;
                    current_occupancy = occupancy_map(&current);
                    current_metrics = swap_metrics;
                    if current_metrics.total < best_metrics.total {
                        best = current.clone();
                        best_metrics = current_metrics.clone();
                    }
                }
            }
            stall = 0;
        }

        temperature *= if step % 40 == 0 { 0.985 } else { 0.9985 };
        temperature = temperature.max(0.02);
    }

    Ok(PlacementSolution {
        placements: best,
        metrics: best_metrics,
    })
}

fn focus_weights(context: &SolveContext<'_>) -> Vec<(String, f64)> {
    context
        .movable
        .iter()
        .map(|cluster| {
            let graph_weight = context
                .graph
                .get(cluster)
                .map(|edges| edges.values().sum::<f64>())
                .unwrap_or(0.0);
            let crit_weight = context.criticality.get(cluster).copied().unwrap_or(0.0);
            let weight = match context.options.mode {
                PlaceMode::BoundingBox => 1.0 + graph_weight,
                PlaceMode::TimingDriven => 1.0 + graph_weight + 1.5 * crit_weight,
            };
            (cluster.clone(), weight.max(0.1))
        })
        .collect()
}

fn initial_placement(
    design: &crate::ir::Design,
    graph: &ClusterGraph,
    model: &PlacementModel,
    criticality: &BTreeMap<String, f64>,
    sites: &[(usize, usize)],
    site_set: &BTreeSet<(usize, usize)>,
) -> Result<BTreeMap<String, (usize, usize)>> {
    let mut placements = BTreeMap::<String, (usize, usize)>::new();
    let mut occupied = BTreeSet::<(usize, usize)>::new();

    for cluster in &design.clusters {
        if !cluster.fixed {
            continue;
        }
        let x = cluster
            .x
            .ok_or_else(|| anyhow!("fixed cluster {} is missing x", cluster.name))?;
        let y = cluster
            .y
            .ok_or_else(|| anyhow!("fixed cluster {} is missing y", cluster.name))?;
        if !site_set.contains(&(x, y)) {
            bail!(
                "fixed cluster {} is assigned to non-logic site ({}, {})",
                cluster.name,
                x,
                y
            );
        }
        if !occupied.insert((x, y)) {
            bail!(
                "multiple fixed clusters requested logic site ({}, {})",
                x,
                y
            );
        }
        placements.insert(cluster.name.clone(), (x, y));
    }

    let mut cluster_order = design
        .clusters
        .iter()
        .filter(|cluster| !cluster.fixed)
        .map(|cluster| {
            let graph_weight = graph
                .get(&cluster.name)
                .map(|edges| edges.values().sum::<f64>())
                .unwrap_or(0.0);
            let crit_weight = criticality.get(&cluster.name).copied().unwrap_or(0.0);
            (cluster.name.clone(), graph_weight + crit_weight)
        })
        .collect::<Vec<_>>();
    cluster_order.sort_by(|lhs, rhs| rhs.1.total_cmp(&lhs.1).then_with(|| lhs.0.cmp(&rhs.0)));

    for (cluster_name, _) in cluster_order {
        let target = weighted_centroid(&cluster_name, graph, &placements)
            .or_else(|| model.signal_centroid(&cluster_name, &placements))
            .unwrap_or_else(|| {
                let center = sites[sites.len() / 2];
                (center.0, center.1)
            });
        let site = nearest_free_site(target, sites, &occupied)
            .ok_or_else(|| anyhow!("ran out of logic sites during initial placement"))?;
        occupied.insert(site);
        placements.insert(cluster_name, site);
    }

    Ok(placements)
}

fn choose_focus<'a>(focus_weights: &'a [(String, f64)], rng: &mut ChaCha8Rng) -> Option<&'a str> {
    let total = focus_weights.iter().map(|(_, weight)| *weight).sum::<f64>();
    if total <= 0.0 {
        return focus_weights.first().map(|(name, _)| name.as_str());
    }
    let mut needle = rng.random::<f64>() * total;
    for (name, weight) in focus_weights {
        needle -= *weight;
        if needle <= 0.0 {
            return Some(name.as_str());
        }
    }
    focus_weights.last().map(|(name, _)| name.as_str())
}

fn candidate_targets(
    focus: &str,
    model: &PlacementModel,
    graph: &ClusterGraph,
    placements: &BTreeMap<String, (usize, usize)>,
    sites: &[(usize, usize)],
    site_set: &BTreeSet<(usize, usize)>,
    rng: &mut ChaCha8Rng,
) -> Vec<(usize, usize)> {
    let mut targets = BTreeSet::<(usize, usize)>::new();
    if let Some(current) = placements.get(focus) {
        targets.insert(*current);
        extend_best_sites(*current, sites, 3, &mut targets);
    }

    if let Some(centroid) = weighted_centroid(focus, graph, placements) {
        extend_best_sites(centroid, sites, 5, &mut targets);
    }
    if let Some(signal_center) = model.signal_centroid(focus, placements) {
        extend_best_sites(signal_center, sites, 4, &mut targets);
    }
    if let Some(neighbors) = graph.get(focus) {
        let mut ranked_neighbors = neighbors.iter().collect::<Vec<_>>();
        ranked_neighbors.sort_by(|lhs, rhs| rhs.1.total_cmp(lhs.1).then_with(|| lhs.0.cmp(rhs.0)));
        for (neighbor, _) in ranked_neighbors.into_iter().take(3) {
            if let Some(point) = placements.get(neighbor) {
                targets.insert(*point);
                for nearby in nearby_sites(*point, site_set, 1) {
                    targets.insert(nearby);
                }
            }
        }
    }

    for _ in 0..3 {
        let site = sites[rng.random_range(0..sites.len())];
        targets.insert(site);
    }

    targets.into_iter().collect()
}

fn extend_best_sites(
    target: (usize, usize),
    sites: &[(usize, usize)],
    limit: usize,
    out: &mut BTreeSet<(usize, usize)>,
) {
    if limit == 0 {
        return;
    }

    let mut ranked = Vec::<((usize, usize), usize)>::new();
    for site in sites {
        let distance = manhattan(*site, target);
        let insert_at = ranked
            .iter()
            .position(|(candidate, candidate_distance)| {
                (*candidate_distance, *candidate) > (distance, *site)
            })
            .unwrap_or(ranked.len());
        if insert_at < limit {
            ranked.insert(insert_at, (*site, distance));
            if ranked.len() > limit {
                ranked.pop();
            }
        } else if ranked.len() < limit {
            ranked.push((*site, distance));
        }
    }

    for (site, _) in ranked {
        out.insert(site);
    }
}

fn nearby_sites(
    center: (usize, usize),
    site_set: &BTreeSet<(usize, usize)>,
    radius: usize,
) -> Vec<(usize, usize)> {
    let min_x = center.0.saturating_sub(radius);
    let min_y = center.1.saturating_sub(radius);
    let max_x = center.0 + radius;
    let max_y = center.1 + radius;
    let mut result = Vec::new();
    for x in min_x..=max_x {
        for y in min_y..=max_y {
            if site_set.contains(&(x, y)) {
                result.push((x, y));
            }
        }
    }
    result.sort_unstable_by(|lhs, rhs| {
        manhattan(*lhs, center)
            .cmp(&manhattan(*rhs, center))
            .then_with(|| lhs.cmp(rhs))
    });
    result
}

fn plan_target_updates(
    placements: &BTreeMap<String, (usize, usize)>,
    occupancy: &BTreeMap<(usize, usize), String>,
    movable_set: &BTreeSet<String>,
    focus: &str,
    target: (usize, usize),
) -> Option<Vec<(String, (usize, usize))>> {
    let current = *placements.get(focus)?;
    if current == target {
        return Some(Vec::new());
    }

    let occupant = occupancy
        .get(&target)
        .filter(|cluster| cluster.as_str() != focus)
        .cloned();

    if let Some(occupant) = occupant {
        if !movable_set.contains(&occupant) {
            return None;
        }
        Some(vec![(focus.to_string(), target), (occupant, current)])
    } else {
        Some(vec![(focus.to_string(), target)])
    }
}

fn occupancy_map(
    placements: &BTreeMap<String, (usize, usize)>,
) -> BTreeMap<(usize, usize), String> {
    placements
        .iter()
        .map(|(cluster, position)| (*position, cluster.clone()))
        .collect()
}

fn apply_updates(
    placements: &BTreeMap<String, (usize, usize)>,
    updates: &[(String, (usize, usize))],
) -> BTreeMap<String, (usize, usize)> {
    let mut trial = placements.clone();
    for (cluster, position) in updates {
        trial.insert(cluster.clone(), *position);
    }
    trial
}

fn random_swap_updates(
    placements: &BTreeMap<String, (usize, usize)>,
    movable: &[String],
    rng: &mut ChaCha8Rng,
) -> Option<Vec<(String, (usize, usize))>> {
    if movable.len() < 2 {
        return None;
    }
    let lhs_index = rng.random_range(0..movable.len());
    let mut rhs_index = rng.random_range(0..movable.len());
    while rhs_index == lhs_index {
        rhs_index = rng.random_range(0..movable.len());
    }
    let lhs = &movable[lhs_index];
    let rhs = &movable[rhs_index];
    let lhs_pos = *placements.get(lhs)?;
    let rhs_pos = *placements.get(rhs)?;
    Some(vec![(lhs.clone(), rhs_pos), (rhs.clone(), lhs_pos)])
}

fn nearest_free_site(
    target: (usize, usize),
    sites: &[(usize, usize)],
    occupied: &BTreeSet<(usize, usize)>,
) -> Option<(usize, usize)> {
    sites
        .iter()
        .filter(|site| !occupied.contains(site))
        .min_by(|lhs, rhs| {
            manhattan(**lhs, target)
                .cmp(&manhattan(**rhs, target))
                .then_with(|| lhs.cmp(rhs))
        })
        .copied()
}
