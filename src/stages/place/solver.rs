use crate::{
    domain::ClusterKind,
    ir::ClusterId,
    place::{PlaceMode, PlaceOptions},
    report::{
        ProgressUpdate, StageReporter, WorkUnit, emit_stage_info, emit_stage_progress_update,
    },
};
use anyhow::{Result, anyhow};
use rand::RngExt;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use super::{
    cost::{PlacementCandidate, PlacementEvaluator, PlacementMetrics, evaluate_positions},
    graph::{ClusterGraph, build_cluster_graph, cluster_incident_criticality},
    model::{PlacementModel, Point},
    support::{
        CandidateTargets, ClusterUpdates, FocusSampler, OccupancyMap, SiteOccupancy,
        TargetUpdateContext, apply_updates_in_place, best_neighbors, candidate_targets,
        choose_focus, extend_best_sites, initial_placement, nearby_sites, occupancy_map,
        plan_target_updates, push_unique, random_swap_updates, restore_updates, site_mask,
    },
};

const INCREMENTAL_EVALUATOR_NET_THRESHOLD: usize = 128;
const CANDIDATE_SCORE_PARALLEL_THRESHOLD: usize = 16;
const CANDIDATE_SCORE_PARALLEL_MIN_MOVABLE_CLUSTERS: usize = 1024;
const ANNEAL_TEMPERATURE_FLOOR: f64 = 0.02;
const PLATEAU_EARLY_EXIT_MIN_ITERATIONS: usize = 50_000;
const PLATEAU_EARLY_EXIT_MIN_MOVABLE_CLUSTERS: usize = 512;
const PLATEAU_EARLY_EXIT_MIN_COMPLETION_NUMERATOR: usize = 3;
const PLATEAU_EARLY_EXIT_MIN_COMPLETION_DENOMINATOR: usize = 5;
const PLATEAU_EARLY_EXIT_RELATIVE_IMPROVEMENT: f64 = 0.0005;

/// Trials between adaptive move-weight rebalancing passes.
const ADAPTIVE_REBALANCE_INTERVAL: usize = 128;
/// Clamp for the swap-vs-relocate preference so neither kind starves.
const MOVE_WEIGHT_FLOOR: f64 = 0.1;
/// Rolling acceptance window used by the gentle-reheat heuristic.
const ACCEPTANCE_WINDOW: u32 = 256;
/// Below this window acceptance ratio, mildly reheat once to escape shallow
/// local minima instead of freezing in place.
const REHEAT_ACCEPTANCE_FLOOR: f64 = 0.04;
const REHEAT_FACTOR: f64 = 1.10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MoveKind {
    Swap,
    Relocate,
}

/// Adaptive move-kind weighting plus rolling acceptance statistics.
#[derive(Debug)]
struct AdaptiveMoves {
    swap_weight: f64,
    attempts: [u32; 2],
    accepts: [u32; 2],
    since_rebalance: usize,
    window_attempts: u32,
    window_accepts: u32,
    /// Set when an acceptance window closes; consumed by the scheduler.
    pending_window_ratio: Option<f64>,
}

impl Default for AdaptiveMoves {
    fn default() -> Self {
        Self {
            swap_weight: 0.5,
            attempts: [0; 2],
            accepts: [0; 2],
            since_rebalance: 0,
            window_attempts: 0,
            window_accepts: 0,
            pending_window_ratio: None,
        }
    }
}

impl AdaptiveMoves {
    fn pick_kind(&self, rng: &mut ChaCha8Rng) -> MoveKind {
        if rng.random::<f64>() < self.swap_weight {
            MoveKind::Swap
        } else {
            MoveKind::Relocate
        }
    }

    fn record(&mut self, kind: MoveKind, accepted: bool) {
        let slot = match kind {
            MoveKind::Swap => 0,
            MoveKind::Relocate => 1,
        };
        self.attempts[slot] += 1;
        self.window_attempts += 1;
        if accepted {
            self.accepts[slot] += 1;
            self.window_accepts += 1;
        }
        self.since_rebalance += 1;

        if self.since_rebalance >= ADAPTIVE_REBALANCE_INTERVAL {
            self.rebalance();
        }
        if self.window_attempts >= ACCEPTANCE_WINDOW {
            self.pending_window_ratio =
                Some(f64::from(self.window_accepts) / f64::from(self.window_attempts));
            self.window_attempts = 0;
            self.window_accepts = 0;
        }
    }

    /// Acceptance ratio of the last closed window, if any.
    fn take_window_ratio(&mut self) -> Option<f64> {
        self.pending_window_ratio.take()
    }

    /// VPR-style rebalance: shift weight toward the move kind with the higher
    /// recent acceptance ratio, clamped so both kinds stay alive.
    fn rebalance(&mut self) {
        self.since_rebalance = 0;
        let rate = |slot: usize| {
            let attempts = f64::from(self.attempts[slot]);
            if attempts <= 0.0 {
                return 0.0;
            }
            f64::from(self.accepts[slot]) / attempts
        };
        let swap_rate = rate(0);
        let relocate_rate = rate(1);
        if swap_rate + relocate_rate <= f64::EPSILON {
            return;
        }
        let target = swap_rate / (swap_rate + relocate_rate);
        self.swap_weight = target
            .clamp(1.0 - MOVE_WEIGHT_FLOOR * 9.0, MOVE_WEIGHT_FLOOR * 9.0)
            .clamp(MOVE_WEIGHT_FLOOR, 1.0 - MOVE_WEIGHT_FLOOR);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PlacementSolution {
    pub(crate) placements: Vec<Option<Point>>,
    pub(crate) metrics: PlacementMetrics,
}

struct SolveContext<'a> {
    design: &'a crate::ir::Design,
    options: &'a PlaceOptions,
    graph: &'a ClusterGraph,
    model: &'a PlacementModel,
    criticality: &'a [f64],
    logic_sites: &'a [Point],
    logic_site_mask: &'a [bool],
    block_ram_sites: &'a [Point],
    block_ram_site_mask: &'a [bool],
    logic_site_capacity: usize,
    movable: &'a [ClusterId],
    movable_mask: &'a [bool],
}

struct IncrementalAnnealState<'a> {
    evaluator: PlacementEvaluator<'a>,
    occupancy: OccupancyMap,
    metrics: PlacementMetrics,
}

struct FullAnnealState {
    current: Vec<Option<Point>>,
    trial: Vec<Option<Point>>,
    occupancy: OccupancyMap,
    metrics: PlacementMetrics,
}

#[derive(Debug, Clone)]
struct PlateauExitState {
    enabled: bool,
    min_completion_step: usize,
    window: usize,
    window_start_step: Option<usize>,
    window_start_best_total: f64,
}

pub(crate) fn solve(
    design: &crate::ir::Design,
    options: &PlaceOptions,
) -> Result<PlacementSolution> {
    let mut logger = None;
    solve_internal(design, options, &mut logger, None)
}

pub(crate) fn solve_with_reporter(
    design: &crate::ir::Design,
    options: &PlaceOptions,
    reporter: &mut dyn StageReporter,
) -> Result<PlacementSolution> {
    solve_internal(design, options, &mut Some(reporter), None)
}

fn solve_internal(
    design: &crate::ir::Design,
    options: &PlaceOptions,
    reporter: &mut Option<&mut dyn StageReporter>,
    incremental_override: Option<bool>,
) -> Result<PlacementSolution> {
    let logic_sites = options
        .arch
        .logic_sites()
        .into_iter()
        .map(Point::from)
        .collect::<Vec<_>>();
    let logic_site_mask = site_mask(&logic_sites, options.arch.width, options.arch.height);
    let block_ram_sites = options
        .arch
        .block_ram_sites()
        .into_iter()
        .map(Point::from)
        .collect::<Vec<_>>();
    let block_ram_site_mask = site_mask(&block_ram_sites, options.arch.width, options.arch.height);
    let logic_site_capacity = options.arch.slices_per_tile.max(1);
    let graph = build_cluster_graph(design);
    let model = PlacementModel::from_design(design);
    let criticality = cluster_incident_criticality(design);
    let movable = design
        .clusters
        .iter()
        .enumerate()
        .filter(|(_, cluster)| !cluster.fixed)
        .map(|(index, _)| ClusterId::new(index))
        .collect::<Vec<_>>();
    let mut movable_mask = vec![false; design.clusters.len()];
    for cluster_id in &movable {
        movable_mask[cluster_id.index()] = true;
    }

    if movable.len() <= 1 {
        let current = initial_placement(
            design,
            &graph,
            &model,
            &criticality,
            &logic_sites,
            &logic_site_mask,
            &block_ram_sites,
            &block_ram_site_mask,
            options.arch.width,
            options.arch.height,
            logic_site_capacity,
        )?;
        let metrics = evaluate_positions(
            &model,
            &graph,
            &current,
            &options.arch,
            options.delay.as_deref(),
            options.mode,
        );
        return Ok(PlacementSolution {
            placements: current,
            metrics,
        });
    }

    let context = SolveContext {
        design,
        options,
        graph: &graph,
        model: &model,
        criticality: &criticality,
        logic_sites: &logic_sites,
        logic_site_mask: &logic_site_mask,
        block_ram_sites: &block_ram_sites,
        block_ram_site_mask: &block_ram_site_mask,
        logic_site_capacity,
        movable: &movable,
        movable_mask: &movable_mask,
    };

    let use_incremental =
        incremental_override.unwrap_or(model.nets.len() >= INCREMENTAL_EVALUATOR_NET_THRESHOLD);
    emit_stage_info(
        reporter,
        "place",
        format!(
            "placement solver initialized: movable_clusters={}, nets={}, strategy={}",
            movable.len(),
            model.nets.len(),
            if use_incremental {
                "incremental"
            } else {
                "full"
            }
        ),
    );
    if use_incremental {
        solve_incremental(&context, reporter)
    } else {
        solve_full(&context, reporter)
    }
}

fn should_log_progress(step: usize, iterations: usize) -> bool {
    if iterations <= 20 {
        return true;
    }
    let interval = (iterations / 20).max(1);
    step == 0 || step + 1 == iterations || (step + 1).is_multiple_of(interval)
}

fn emit_anneal_progress(
    reporter: &mut Option<&mut dyn StageReporter>,
    strategy: &str,
    step: usize,
    iterations: usize,
    temperature: f64,
    current: &PlacementMetrics,
    best: &PlacementMetrics,
) {
    emit_stage_progress_update(
        reporter,
        "place",
        ProgressUpdate::new(
            format!("{strategy} anneal"),
            step + 1,
            iterations,
            WorkUnit::Iterations,
        )
        .metric("temp", format!("{temperature:.3}"))
        .metric("current", format!("{:.3}", current.total))
        .metric("best", format!("{:.3}", best.total)),
    );
}

fn initial_positions(context: &SolveContext<'_>) -> Result<Vec<Option<Point>>> {
    initial_placement(
        context.design,
        context.graph,
        context.model,
        context.criticality,
        context.logic_sites,
        context.logic_site_mask,
        context.block_ram_sites,
        context.block_ram_site_mask,
        context.options.arch.width,
        context.options.arch.height,
        context.logic_site_capacity,
    )
}

fn anneal_iterations(context: &SolveContext<'_>) -> usize {
    700 + context.movable.len() * 50
}

fn anneal_temperature(total_cost: f64, movable_count: usize) -> f64 {
    (total_cost / movable_count.max(1) as f64).max(0.5)
}

fn cool_temperature(temperature: f64, step: usize) -> f64 {
    let cooled = temperature
        * if step.is_multiple_of(40) {
            0.985
        } else {
            0.9985
        };
    cooled.max(ANNEAL_TEMPERATURE_FLOOR)
}

fn stall_limit(context: &SolveContext<'_>) -> usize {
    context.movable.len() * 3
}

impl PlateauExitState {
    fn new(iterations: usize, movable_count: usize, best_total: f64) -> Self {
        let enabled = iterations >= PLATEAU_EARLY_EXIT_MIN_ITERATIONS
            && movable_count >= PLATEAU_EARLY_EXIT_MIN_MOVABLE_CLUSTERS;
        let min_completion_step = plateau_min_completion_step(iterations);
        let window = (iterations / 20)
            .max(movable_count * 2)
            .min(iterations.max(1));
        Self {
            enabled,
            min_completion_step,
            window,
            window_start_step: None,
            window_start_best_total: best_total,
        }
    }

    fn should_stop(&mut self, step: usize, temperature: f64, best_total: f64) -> bool {
        if !self.enabled
            || step + 1 < self.min_completion_step
            || temperature > ANNEAL_TEMPERATURE_FLOOR + f64::EPSILON
        {
            self.window_start_step = None;
            self.window_start_best_total = best_total;
            return false;
        }

        let Some(window_start_step) = self.window_start_step else {
            self.window_start_step = Some(step + 1);
            self.window_start_best_total = best_total;
            return false;
        };

        if step + 1 - window_start_step < self.window {
            return false;
        }

        let baseline = self.window_start_best_total.max(1.0);
        let relative_improvement =
            ((self.window_start_best_total - best_total).max(0.0)) / baseline;
        self.window_start_step = Some(step + 1);
        self.window_start_best_total = best_total;
        relative_improvement < PLATEAU_EARLY_EXIT_RELATIVE_IMPROVEMENT
    }
}

fn plateau_min_completion_step(iterations: usize) -> usize {
    let whole = iterations / PLATEAU_EARLY_EXIT_MIN_COMPLETION_DENOMINATOR
        * PLATEAU_EARLY_EXIT_MIN_COMPLETION_NUMERATOR;
    let remainder = iterations % PLATEAU_EARLY_EXIT_MIN_COMPLETION_DENOMINATOR
        * PLATEAU_EARLY_EXIT_MIN_COMPLETION_NUMERATOR;
    whole + remainder.div_ceil(PLATEAU_EARLY_EXIT_MIN_COMPLETION_DENOMINATOR)
}

fn choose_focus_and_targets(
    context: &SolveContext<'_>,
    placements: &[Option<Point>],
    focus_sampler: &FocusSampler,
    rng: &mut ChaCha8Rng,
    move_kind: MoveKind,
) -> Result<(ClusterId, CandidateTargets)> {
    let focus = choose_focus(focus_sampler, rng)
        .ok_or_else(|| anyhow!("missing movable cluster during placement"))?;
    let (sites, site_mask, _) = site_resources(context, focus);
    let targets = candidate_targets(
        focus,
        cluster_kind(context, focus),
        context.model,
        context.graph,
        placements,
        sites,
        site_mask,
        context.options.arch.width,
        context.options.arch.height,
        rng,
        move_kind,
    );
    Ok((focus, targets))
}

fn accept_trial(
    rng: &mut ChaCha8Rng,
    current_total: f64,
    trial_total: f64,
    temperature: f64,
) -> bool {
    if trial_total + 1e-9 < current_total {
        return true;
    }
    let delta = trial_total - current_total;
    let threshold = (-delta / temperature.max(0.01)).exp().clamp(0.0, 1.0);
    rng.random::<f64>() < threshold
}

fn update_best_solution(
    best: &mut PlacementSolution,
    current: &[Option<Point>],
    current_metrics: &PlacementMetrics,
) -> bool {
    if current_metrics.total + 1e-9 >= best.metrics.total {
        return false;
    }
    best.placements.as_mut_slice().clone_from_slice(current);
    best.metrics = current_metrics.clone();
    true
}

fn incremental_state<'a>(
    context: &'a SolveContext<'a>,
    placements: Vec<Option<Point>>,
) -> IncrementalAnnealState<'a> {
    let evaluator = PlacementEvaluator::new_from_positions(
        context.model,
        context.graph,
        placements,
        &context.options.arch,
        context.options.delay.as_deref(),
        context.options.mode,
    );
    let occupancy = occupancy_map(
        evaluator.placements(),
        context.options.arch.width,
        context.options.arch.height,
    );
    let metrics = evaluator.metrics().clone();
    IncrementalAnnealState {
        evaluator,
        occupancy,
        metrics,
    }
}

fn full_state(context: &SolveContext<'_>, current: Vec<Option<Point>>) -> FullAnnealState {
    let occupancy = occupancy_map(
        &current,
        context.options.arch.width,
        context.options.arch.height,
    );
    let metrics = evaluate_positions(
        context.model,
        context.graph,
        &current,
        &context.options.arch,
        context.options.delay.as_deref(),
        context.options.mode,
    );
    let trial = current.clone();
    FullAnnealState {
        current,
        trial,
        occupancy,
        metrics,
    }
}

fn best_incremental_trial(
    context: &SolveContext<'_>,
    evaluator: &mut PlacementEvaluator<'_>,
    current_occupancy: &[SiteOccupancy],
    focus: ClusterId,
    candidates: CandidateTargets,
) -> Option<PlacementCandidate> {
    let (_, site_mask, site_capacity) = site_resources(context, focus);
    if !should_parallel_score(context, candidates.len()) {
        let mut best_trial: Option<(usize, ClusterUpdates, f64)> = None;
        for (index, target) in candidates.into_iter().enumerate() {
            let Some(changes) = plan_target_updates(
                TargetUpdateContext {
                    placements: evaluator.placements(),
                    occupancy: current_occupancy,
                    movable_mask: context.movable_mask,
                    site_mask,
                    width: context.options.arch.width,
                    height: context.options.arch.height,
                    site_capacity,
                },
                focus,
                target,
            ) else {
                continue;
            };
            let metrics = evaluator.evaluate_prepared_candidate_metrics(&changes);
            let replace = best_trial
                .as_ref()
                .is_none_or(|(best_index, _, best_total)| {
                    best_metric_choice(Some((*best_index, *best_total)), (index, metrics.total)).0
                        == index
                });
            if replace {
                best_trial = Some((index, changes, metrics.total));
            }
        }
        return best_trial.map(|(_, changes, _)| evaluator.evaluate_prepared_candidate(changes));
    }

    let mut planned = Vec::with_capacity(candidates.len());
    for target in candidates {
        let Some(changes) = plan_target_updates(
            TargetUpdateContext {
                placements: evaluator.placements(),
                occupancy: current_occupancy,
                movable_mask: context.movable_mask,
                site_mask,
                width: context.options.arch.width,
                height: context.options.arch.height,
                site_capacity,
            },
            focus,
            target,
        ) else {
            continue;
        };
        planned.push(changes);
    }

    if planned.is_empty() {
        return None;
    }

    let best_index = if planned.len() >= CANDIDATE_SCORE_PARALLEL_THRESHOLD {
        evaluator
            .best_candidate_metrics_parallel(&planned)
            .map(|(index, _)| index)?
    } else {
        let mut best_choice: Option<(usize, f64)> = None;
        for (index, changes) in planned.iter().enumerate() {
            let metrics = evaluator.evaluate_prepared_candidate_metrics(changes);
            best_choice = Some(best_metric_choice(best_choice, (index, metrics.total)));
        }
        best_choice.map(|(index, _)| index)?
    };

    planned
        .get(best_index)
        .cloned()
        .map(|changes| evaluator.evaluate_prepared_candidate(changes))
}

fn maybe_apply_incremental_swap(
    context: &SolveContext<'_>,
    state: &mut IncrementalAnnealState<'_>,
    best: &mut PlacementSolution,
    rng: &mut ChaCha8Rng,
) {
    if let Some(swapped) = random_swap_updates(
        context.design,
        state.evaluator.placements(),
        context.movable,
        rng,
    ) {
        let swap_metrics = state
            .evaluator
            .evaluate_prepared_candidate_metrics(&swapped);
        if swap_metrics.total < state.metrics.total {
            let swap_candidate = state.evaluator.evaluate_prepared_candidate(swapped);
            state.evaluator.apply_candidate(swap_candidate);
            state.occupancy = occupancy_map(
                state.evaluator.placements(),
                context.options.arch.width,
                context.options.arch.height,
            );
            state.metrics = swap_metrics;
            update_best_solution(best, state.evaluator.placements(), &state.metrics);
        }
    }
}

fn solve_incremental(
    context: &SolveContext<'_>,
    reporter: &mut Option<&mut dyn StageReporter>,
) -> Result<PlacementSolution> {
    let mut rng = ChaCha8Rng::seed_from_u64(context.options.seed);
    let mut state = incremental_state(context, initial_positions(context)?);
    let mut best = PlacementSolution {
        placements: state.evaluator.placements().to_vec(),
        metrics: state.evaluator.metrics().clone(),
    };
    let focus_sampler = FocusSampler::new(focus_weights(context));

    let iterations = anneal_iterations(context);
    let mut temperature = anneal_temperature(state.metrics.total, context.movable.len());
    let mut stall = 0usize;
    let mut moves = AdaptiveMoves::default();
    let mut plateau_exit =
        PlateauExitState::new(iterations, context.movable.len(), best.metrics.total);
    emit_stage_info(
        reporter,
        "place",
        format!(
            "starting incremental anneal with {} iterations, initial cost {:.3}",
            iterations, state.metrics.total
        ),
    );

    for step in 0..iterations {
        if should_log_progress(step, iterations) {
            emit_anneal_progress(
                reporter,
                "incremental",
                step,
                iterations,
                temperature,
                &state.metrics,
                &best.metrics,
            );
        }
        let move_kind = moves.pick_kind(&mut rng);
        let (focus, candidates) = choose_focus_and_targets(
            context,
            state.evaluator.placements(),
            &focus_sampler,
            &mut rng,
            move_kind,
        )?;
        let best_trial = best_incremental_trial(
            context,
            &mut state.evaluator,
            &state.occupancy,
            focus,
            candidates,
        );

        let Some(trial) = best_trial else {
            moves.record(move_kind, false);
            continue;
        };
        let trial_metrics = trial.metrics().clone();
        let accept = accept_trial(
            &mut rng,
            state.metrics.total,
            trial_metrics.total,
            temperature,
        );
        moves.record(move_kind, accept);

        if accept {
            state.evaluator.apply_candidate(trial);
            state.occupancy = occupancy_map(
                state.evaluator.placements(),
                context.options.arch.width,
                context.options.arch.height,
            );
            state.metrics = trial_metrics;
            if update_best_solution(&mut best, state.evaluator.placements(), &state.metrics) {
                stall = 0;
            } else {
                stall += 1;
            }
        } else {
            stall += 1;
        }

        if stall > stall_limit(context) {
            maybe_apply_incremental_swap(context, &mut state, &mut best, &mut rng);
            stall = 0;
        }

        temperature = cool_temperature(temperature, step);
        if moves
            .take_window_ratio()
            .is_some_and(|ratio| ratio < REHEAT_ACCEPTANCE_FLOOR)
        {
            // Gentle reheat: the annealer froze mid-run, back off slightly.
            temperature = (temperature * REHEAT_FACTOR).min(anneal_temperature(
                state.metrics.total,
                context.movable.len(),
            ));
        }
        if plateau_exit.should_stop(step, temperature, best.metrics.total) {
            emit_stage_info(
                reporter,
                "place",
                format!(
                    "stopping incremental anneal early at {}/{} after plateau at temperature floor; best cost {:.3}",
                    step + 1,
                    iterations,
                    best.metrics.total
                ),
            );
            break;
        }
    }

    emit_stage_info(
        reporter,
        "place",
        format!(
            "incremental anneal finished with best cost {:.3}; starting refinement",
            best.metrics.total
        ),
    );
    let metrics = best.metrics;
    Ok(refine_solution(
        context,
        best.placements,
        &metrics,
        reporter,
    ))
}

fn best_full_trial(
    context: &SolveContext<'_>,
    current: &[Option<Point>],
    trial: &mut [Option<Point>],
    current_occupancy: &[SiteOccupancy],
    focus: ClusterId,
    candidates: CandidateTargets,
) -> Option<(ClusterUpdates, PlacementMetrics)> {
    let mut best_trial: Option<(usize, ClusterUpdates, PlacementMetrics)> = None;
    let (_, site_mask, site_capacity) = site_resources(context, focus);
    for (index, target) in candidates.into_iter().enumerate() {
        let Some(changes) = plan_target_updates(
            TargetUpdateContext {
                placements: current,
                occupancy: current_occupancy,
                movable_mask: context.movable_mask,
                site_mask,
                width: context.options.arch.width,
                height: context.options.arch.height,
                site_capacity,
            },
            focus,
            target,
        ) else {
            continue;
        };
        let backups = apply_updates_in_place(trial, &changes);
        let metrics = evaluate_positions(
            context.model,
            context.graph,
            trial,
            &context.options.arch,
            context.options.delay.as_deref(),
            context.options.mode,
        );
        restore_updates(trial, &backups);
        let replace = best_trial
            .as_ref()
            .is_none_or(|(best_index, _, best_metrics)| {
                best_metric_choice(
                    Some((*best_index, best_metrics.total)),
                    (index, metrics.total),
                )
                .0 == index
            });
        if replace {
            best_trial = Some((index, changes, metrics));
        }
    }
    best_trial.map(|(_, changes, metrics)| (changes, metrics))
}

fn maybe_apply_full_swap(
    context: &SolveContext<'_>,
    state: &mut FullAnnealState,
    best: &mut PlacementSolution,
    rng: &mut ChaCha8Rng,
) {
    if let Some(swapped) = random_swap_updates(context.design, &state.current, context.movable, rng)
    {
        let backups = apply_updates_in_place(&mut state.trial, &swapped);
        let swap_metrics = evaluate_positions(
            context.model,
            context.graph,
            &state.trial,
            &context.options.arch,
            context.options.delay.as_deref(),
            context.options.mode,
        );
        restore_updates(&mut state.trial, &backups);
        if swap_metrics.total < state.metrics.total {
            apply_updates_in_place(&mut state.current, &swapped);
            apply_updates_in_place(&mut state.trial, &swapped);
            state.occupancy = occupancy_map(
                &state.current,
                context.options.arch.width,
                context.options.arch.height,
            );
            state.metrics = swap_metrics;
            update_best_solution(best, &state.current, &state.metrics);
        }
    }
}

fn solve_full(
    context: &SolveContext<'_>,
    reporter: &mut Option<&mut dyn StageReporter>,
) -> Result<PlacementSolution> {
    let mut rng = ChaCha8Rng::seed_from_u64(context.options.seed);
    let mut state = full_state(context, initial_positions(context)?);
    let mut best = PlacementSolution {
        placements: state.current.clone(),
        metrics: state.metrics.clone(),
    };
    let focus_sampler = FocusSampler::new(focus_weights(context));

    let iterations = anneal_iterations(context);
    let mut temperature = anneal_temperature(state.metrics.total, context.movable.len());
    let mut stall = 0usize;
    let mut moves = AdaptiveMoves::default();
    let mut plateau_exit =
        PlateauExitState::new(iterations, context.movable.len(), best.metrics.total);
    emit_stage_info(
        reporter,
        "place",
        format!(
            "starting full anneal with {} iterations, initial cost {:.3}",
            iterations, state.metrics.total
        ),
    );

    for step in 0..iterations {
        if should_log_progress(step, iterations) {
            emit_anneal_progress(
                reporter,
                "full",
                step,
                iterations,
                temperature,
                &state.metrics,
                &best.metrics,
            );
        }
        let move_kind = moves.pick_kind(&mut rng);
        let (focus, candidates) =
            choose_focus_and_targets(context, &state.current, &focus_sampler, &mut rng, move_kind)?;
        let best_trial = best_full_trial(
            context,
            &state.current,
            &mut state.trial,
            &state.occupancy,
            focus,
            candidates,
        );

        let Some((trial_updates, trial_metrics)) = best_trial else {
            moves.record(move_kind, false);
            continue;
        };
        let accept = accept_trial(
            &mut rng,
            state.metrics.total,
            trial_metrics.total,
            temperature,
        );
        moves.record(move_kind, accept);

        if accept {
            apply_updates_in_place(&mut state.current, &trial_updates);
            apply_updates_in_place(&mut state.trial, &trial_updates);
            state.occupancy = occupancy_map(
                &state.current,
                context.options.arch.width,
                context.options.arch.height,
            );
            state.metrics = trial_metrics;
            if update_best_solution(&mut best, &state.current, &state.metrics) {
                stall = 0;
            } else {
                stall += 1;
            }
        } else {
            stall += 1;
        }

        if stall > stall_limit(context) {
            maybe_apply_full_swap(context, &mut state, &mut best, &mut rng);
            stall = 0;
        }

        temperature = cool_temperature(temperature, step);
        if moves
            .take_window_ratio()
            .is_some_and(|ratio| ratio < REHEAT_ACCEPTANCE_FLOOR)
        {
            // Gentle reheat: the annealer froze mid-run, back off slightly.
            temperature = (temperature * REHEAT_FACTOR).min(anneal_temperature(
                state.metrics.total,
                context.movable.len(),
            ));
        }
        if plateau_exit.should_stop(step, temperature, best.metrics.total) {
            emit_stage_info(
                reporter,
                "place",
                format!(
                    "stopping full anneal early at {}/{} after plateau at temperature floor; best cost {:.3}",
                    step + 1,
                    iterations,
                    best.metrics.total
                ),
            );
            break;
        }
    }

    emit_stage_info(
        reporter,
        "place",
        format!(
            "full anneal finished with best cost {:.3}; starting refinement",
            best.metrics.total
        ),
    );
    let metrics = best.metrics;
    Ok(refine_solution(
        context,
        best.placements,
        &metrics,
        reporter,
    ))
}

fn refine_solution(
    context: &SolveContext<'_>,
    placements: Vec<Option<Point>>,
    metrics: &PlacementMetrics,
    reporter: &mut Option<&mut dyn StageReporter>,
) -> PlacementSolution {
    let mut evaluator = PlacementEvaluator::new_from_positions(
        context.model,
        context.graph,
        placements,
        &context.options.arch,
        context.options.delay.as_deref(),
        context.options.mode,
    );
    if evaluator.metrics().total > metrics.total + 1e-9 {
        return PlacementSolution {
            placements: evaluator.placements().to_vec(),
            metrics: evaluator.metrics().clone(),
        };
    }

    let mut occupancy = occupancy_map(
        evaluator.placements(),
        context.options.arch.width,
        context.options.arch.height,
    );
    let focus_order = refinement_focus_order(context);
    let pass_limit = refinement_pass_limit(context.movable.len());
    emit_stage_info(
        reporter,
        "place",
        format!(
            "refinement configured for up to {} pass(es) across {} focus clusters",
            pass_limit,
            focus_order.len()
        ),
    );

    for pass_index in 0..pass_limit {
        let mut improved = false;
        for &focus in &focus_order {
            let candidates = refinement_targets(context, focus, evaluator.placements());
            let (_, site_mask, site_capacity) = site_resources(context, focus);
            let best_trial = if !should_parallel_score(context, candidates.len()) {
                let mut best_trial: Option<(usize, ClusterUpdates, f64)> = None;
                for (index, target) in candidates.into_iter().enumerate() {
                    let Some(changes) = plan_target_updates(
                        TargetUpdateContext {
                            placements: evaluator.placements(),
                            occupancy: &occupancy,
                            movable_mask: context.movable_mask,
                            site_mask,
                            width: context.options.arch.width,
                            height: context.options.arch.height,
                            site_capacity,
                        },
                        focus,
                        target,
                    ) else {
                        continue;
                    };
                    if changes.is_empty() {
                        continue;
                    }
                    let metrics = evaluator.evaluate_prepared_candidate_metrics(&changes);
                    if metrics.total + 1e-9 >= evaluator.metrics().total {
                        continue;
                    }
                    let replace = best_trial
                        .as_ref()
                        .is_none_or(|(best_index, _, best_total)| {
                            best_metric_choice(
                                Some((*best_index, *best_total)),
                                (index, metrics.total),
                            )
                            .0 == index
                        });
                    if replace {
                        best_trial = Some((index, changes, metrics.total));
                    }
                }
                best_trial.map(|(_, changes, _)| evaluator.evaluate_prepared_candidate(changes))
            } else {
                let mut planned = Vec::with_capacity(candidates.len());
                for target in candidates {
                    let Some(changes) = plan_target_updates(
                        TargetUpdateContext {
                            placements: evaluator.placements(),
                            occupancy: &occupancy,
                            movable_mask: context.movable_mask,
                            site_mask,
                            width: context.options.arch.width,
                            height: context.options.arch.height,
                            site_capacity,
                        },
                        focus,
                        target,
                    ) else {
                        continue;
                    };
                    if changes.is_empty() {
                        continue;
                    }
                    planned.push(changes);
                }

                if planned.is_empty() {
                    None
                } else {
                    let best_index = if planned.len() >= CANDIDATE_SCORE_PARALLEL_THRESHOLD {
                        evaluator
                            .best_candidate_metrics_parallel(&planned)
                            .map(|(index, _)| index)
                    } else {
                        let mut best_choice: Option<(usize, f64)> = None;
                        for (index, changes) in planned.iter().enumerate() {
                            let metrics = evaluator.evaluate_prepared_candidate_metrics(changes);
                            best_choice =
                                Some(best_metric_choice(best_choice, (index, metrics.total)));
                        }
                        best_choice.map(|(index, _)| index)
                    };

                    best_index.and_then(|index| {
                        planned
                            .get(index)
                            .cloned()
                            .map(|changes| evaluator.evaluate_prepared_candidate(changes))
                    })
                }
            };

            if let Some(trial) = best_trial {
                if trial.metrics().total + 1e-9 >= evaluator.metrics().total {
                    continue;
                }
                evaluator.apply_candidate(trial);
                occupancy = occupancy_map(
                    evaluator.placements(),
                    context.options.arch.width,
                    context.options.arch.height,
                );
                improved = true;
            }
        }
        emit_stage_progress_update(
            reporter,
            "place",
            ProgressUpdate::new("refinement", pass_index + 1, pass_limit, WorkUnit::Passes)
                .metric("state", if improved { "improved" } else { "stable" })
                .metric("cost", format!("{:.3}", evaluator.metrics().total)),
        );
        if !improved {
            break;
        }
    }

    emit_stage_info(
        reporter,
        "place",
        format!(
            "placement refinement complete with final cost {:.3}",
            evaluator.metrics().total
        ),
    );
    PlacementSolution {
        placements: evaluator.placements().to_vec(),
        metrics: evaluator.metrics().clone(),
    }
}

fn best_metric_choice(current: Option<(usize, f64)>, candidate: (usize, f64)) -> (usize, f64) {
    match current {
        Some(best) => match candidate.1.total_cmp(&best.1) {
            std::cmp::Ordering::Less => candidate,
            std::cmp::Ordering::Equal if candidate.0 < best.0 => candidate,
            _ => best,
        },
        None => candidate,
    }
}

fn should_parallel_score(context: &SolveContext<'_>, candidate_count: usize) -> bool {
    candidate_count >= CANDIDATE_SCORE_PARALLEL_THRESHOLD
        && context.movable.len() >= CANDIDATE_SCORE_PARALLEL_MIN_MOVABLE_CLUSTERS
}

fn refinement_focus_order(context: &SolveContext<'_>) -> Vec<ClusterId> {
    let mut order = focus_weights(context);
    order.sort_by(|lhs, rhs| rhs.1.total_cmp(&lhs.1).then_with(|| lhs.0.cmp(&rhs.0)));
    order
        .into_iter()
        .map(|(cluster_id, _)| cluster_id)
        .collect()
}

fn refinement_pass_limit(movable_count: usize) -> usize {
    if movable_count <= 16 {
        3
    } else if movable_count <= 96 {
        2
    } else {
        1
    }
}

fn refinement_targets(
    context: &SolveContext<'_>,
    focus: ClusterId,
    placements: &[Option<Point>],
) -> CandidateTargets {
    let mut targets = CandidateTargets::new();
    let Some(current) = placements.get(focus.index()).copied().flatten() else {
        return targets;
    };
    let (sites, site_mask, _) = site_resources(context, focus);
    push_unique(&mut targets, current);
    if cluster_kind(context, focus) == ClusterKind::BlockRam {
        for site in sites {
            push_unique(&mut targets, *site);
        }
        return targets;
    }
    for (nearby, _) in nearby_sites(
        current,
        site_mask,
        context.options.arch.width,
        context.options.arch.height,
        2,
    ) {
        push_unique(&mut targets, nearby);
    }

    if let Some(centroid) = context.graph.weighted_centroid(focus, placements) {
        extend_best_sites(centroid, sites, 4, &mut targets);
    }
    if let Some(signal_center) = context.model.signal_centroid(focus, placements) {
        extend_best_sites(signal_center, sites, 4, &mut targets);
    }
    for (neighbor, _) in best_neighbors(context.graph.neighbors(focus), 4) {
        if let Some(point) = placements.get(neighbor.index()).copied().flatten() {
            push_unique(&mut targets, point);
            for (nearby, _) in nearby_sites(
                point,
                site_mask,
                context.options.arch.width,
                context.options.arch.height,
                1,
            ) {
                push_unique(&mut targets, nearby);
            }
        }
    }
    targets
}

fn focus_weights(context: &SolveContext<'_>) -> Vec<(ClusterId, f64)> {
    context
        .movable
        .iter()
        .map(|cluster_id| {
            let graph_weight = context.graph.total_weight(*cluster_id);
            let crit_weight = context
                .criticality
                .get(cluster_id.index())
                .copied()
                .unwrap_or(0.0);
            let weight = match context.options.mode {
                PlaceMode::BoundingBox => 1.0 + graph_weight,
                PlaceMode::TimingDriven => 1.0 + graph_weight + 1.5 * crit_weight,
            };
            (*cluster_id, weight.max(0.1))
        })
        .collect()
}

fn cluster_kind(context: &SolveContext<'_>, cluster_id: ClusterId) -> ClusterKind {
    context
        .design
        .clusters
        .get(cluster_id.index())
        .map_or(ClusterKind::Unknown, |cluster| cluster.kind)
}

fn site_resources<'a>(
    context: &'a SolveContext<'_>,
    cluster_id: ClusterId,
) -> (&'a [Point], &'a [bool], usize) {
    match cluster_kind(context, cluster_id) {
        ClusterKind::BlockRam => (context.block_ram_sites, context.block_ram_site_mask, 1),
        _ => (
            context.logic_sites,
            context.logic_site_mask,
            context.logic_site_capacity,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ANNEAL_TEMPERATURE_FLOOR, PLATEAU_EARLY_EXIT_MIN_ITERATIONS,
        PLATEAU_EARLY_EXIT_RELATIVE_IMPROVEMENT, PlateauExitState, plateau_min_completion_step,
    };

    #[test]
    fn plateau_exit_stays_disabled_for_small_runs() {
        let mut plateau = PlateauExitState::new(10_000, 256, 100.0);
        for step in 0..10_000 {
            assert!(!plateau.should_stop(step, ANNEAL_TEMPERATURE_FLOOR, 100.0));
        }
    }

    #[test]
    fn plateau_completion_ratio_does_not_overflow() {
        assert_eq!(plateau_min_completion_step(usize::MAX), usize::MAX / 5 * 3);
    }

    #[test]
    fn plateau_exit_triggers_after_large_window_with_tiny_improvement() {
        let iterations = PLATEAU_EARLY_EXIT_MIN_ITERATIONS;
        let mut plateau = PlateauExitState::new(iterations, 1_024, 10_000.0);
        let start_step = plateau_min_completion_step(iterations);
        for step in 0..start_step {
            assert!(!plateau.should_stop(step, ANNEAL_TEMPERATURE_FLOOR, 10_000.0));
        }
        let tiny_improvement = 10_000.0 * (PLATEAU_EARLY_EXIT_RELATIVE_IMPROVEMENT * 0.5);
        let mut triggered = false;
        for step in start_step..iterations {
            if plateau.should_stop(step, ANNEAL_TEMPERATURE_FLOOR, 10_000.0 - tiny_improvement) {
                triggered = true;
                break;
            }
        }
        assert!(triggered);
    }
}

#[cfg(test)]
mod adaptive_tests {
    use super::{AdaptiveMoves, MoveKind, REHEAT_ACCEPTANCE_FLOOR};

    #[test]
    fn same_net_style_repeats_never_shift_weights_without_acceptance() {
        let mut moves = AdaptiveMoves::default();
        for _ in 0..512 {
            moves.record(MoveKind::Swap, false);
        }
        // No accepted trials anywhere: rebalancing must not move the weight.
        assert!((moves.swap_weight - 0.5).abs() < f64::EPSILON);
        // Frozen acceptance triggers the reheat signal on window close.
        let ratio = moves.take_window_ratio();
        assert!(ratio.is_some_and(|r| r < REHEAT_ACCEPTANCE_FLOOR));
    }

    #[test]
    fn rebalance_leans_toward_the_successful_move_kind() {
        let mut moves = AdaptiveMoves::default();
        // Relocations succeed consistently, swaps never do.
        for _ in 0..8 {
            moves.record(MoveKind::Relocate, true);
            moves.record(MoveKind::Swap, false);
        }
        moves.rebalance();
        assert!(moves.swap_weight < 0.5, "swap weight {}", moves.swap_weight);
    }

    #[test]
    fn clamps_keep_both_move_kinds_alive() {
        let mut moves = AdaptiveMoves::default();
        for _ in 0..64 {
            moves.record(MoveKind::Relocate, true);
        }
        moves.rebalance();
        assert!(
            moves.swap_weight >= 0.1,
            "swap weight {}",
            moves.swap_weight
        );
    }
}
