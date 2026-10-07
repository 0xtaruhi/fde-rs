# AGENTS Guide for fde-rs

This repository is the standalone Rust 2024 implementation flow for FDE.

## Repository Facts

- Primary product shape: Rust library first, CLI second.
- Primary executable: `fde`.
- Primary frontend assumption: Yosys produces EDIF; this repo consumes EDIF and downstream IR.
- This repo is independent from the historical C++ monolith. Do not reintroduce old mixed-repository assumptions or a single giant pipeline module.
- Determinism matters: fixed seeds should give reproducible output, even if internal work is parallelized.

## Architectural Direction

- Shared typed IR lives in Rust and is reused across all stages.
- Stage logic belongs in focused modules like `map`, `pack`, `place`, `route`, `sta`, `bitgen`, `normalize`, `orchestrator`.
- CLI code should stay thin: argument parsing, file orchestration, report writing, progress output.
- Follow the refactor plan in `docs/refactor-plan.md` when reshaping modules.
- Keep `src/` top-level compact by grouping modules under `app/`, `core/`, `infra/`, and `stages/` instead of adding more root directories.

## Scope Boundaries

- Verilog import is intentionally minimal. Prefer failing clearly and telling the user to run Yosys.
- `bitgen` materializes CIL-backed site SRAM images for supported logic/IO/clock sites and stays within the Rust implementation flow.
- Hardware XML compatibility matters. Reuse established FDE hardware XML conventions and invocation shapes where practical.

## Commands

- Build: `cargo build`
- Check: `cargo check`
- Test: `cargo test`
- CI parity: `cargo fmt --all -- --check && cargo check --locked --all-targets && cargo clippy --locked --all-targets --all-features -- -D warnings && cargo test --locked --quiet`
- Board probe CI: `cargo clippy --locked --manifest-path tools/wave_probe/Cargo.toml --all-targets -- -D warnings && cargo test --locked --manifest-path tools/wave_probe/Cargo.toml`
- CI smoke: `cargo run --locked --quiet --bin fde -- impl --input examples/blinky/blinky.edf --constraints examples/blinky/constraints.xml --resource-root resources/hw_lib --out-dir /tmp/fde-rs-ci-smoke`
- Board EDF dry run: `find examples/board-e2e -mindepth 2 -maxdepth 2 -name '*.edf' | sort | while read -r edf; do case_dir=$(dirname "${edf}"); name=$(basename "${case_dir}"); cargo run --bin fde -- impl --input "${edf}" --constraints "${case_dir}/constraints.xml" --resource-root resources/hw_lib --out-dir "build/board-e2e/${name}"; done`
- Live board run: `python3 scripts/board_e2e.py run`
- Live board single-case run: `python3 scripts/board_e2e.py run sticky16-check`
- RTL-backed board simulation: `python3 scripts/board_e2e.py simulate`
- RTL-backed live board run: `python3 scripts/board_e2e.py run --rtl-only`
- Board baseline diff: `python3 scripts/board_diff.py run`
- Random board/model diff: `python3 scripts/random_board_diff.py --count 5 --seed 20260322 --keep-going`
- Dense router benchmark: `python3 scripts/generate_dense_benchmark.py --width 192 --synthesize`
- Slice config diff: `python3 scripts/slice_config_diff.py --packed <02-packed.xml> --sidecar <06-output.sidecar.txt>`
- Emit debug sidecar: `cargo run --bin fde -- impl --input <design.edf> --constraints <constraints.xml> --resource-root resources/hw_lib --out-dir build/<run> --emit-sidecar`
- Aspen-style Verilog->EDF synthesis: `python3 scripts/synth_yosys_fde.py --top <top> --out-edf build/<top>.edf <sources...>`
- In-repo board probe: `cargo run --manifest-path tools/wave_probe/Cargo.toml -- <bitstream>`
- Raw board trace: `cargo run --manifest-path tools/wave_probe/Cargo.toml -- --trace-jsonl <trace.jsonl> <bitstream>`
- Main help: `cargo run --bin fde -- --help`
- End-to-end smoke: `cargo run --bin fde -- impl --input examples/blinky/blinky.edf --constraints examples/blinky/constraints.xml --resource-root tests/fixtures/hw_lib --out-dir build/blinky-run`
- Package dry run: `cargo publish --locked --dry-run`
- Tag release: `git tag vX.Y.Z && git push origin vX.Y.Z`

## Editing Guidance

- Keep ASCII unless the file already requires something else.
- Prefer small stage-focused modules over broad refactors that blur responsibilities.
- Do not silently swallow missing resource/config inputs; either derive a safe default or surface a clear error.
- When adding new tooling, update this file and `README.md` in the same change.
- The tag-driven release workflow lives in `.github/workflows/release.yml`; keep `Cargo.toml` version, release tag, and release documentation aligned.
- Release archives are expected to bundle `resources/hw_lib` next to the `fde` binary so prebuilt downloads remain runnable without extra setup.
- Keep checked-in board regression netlists in EDF form under `examples/board-e2e/`; do not commit temporary synthesis-only Verilog there.
- Keep board-specific long-cycle probe overrides in `examples/board-e2e/manifest.json` so regressions remain reproducible from the checked-in manifest.
- Keep string handling at parsing and reporting boundaries; do not add new raw string branching in core stage logic when a typed enum or helper can model it.
- Prefer semantic helper modules in `domain/` over repeating `eq_ignore_ascii_case`, `to_ascii_lowercase`, or string literal matches across stage code.
- Reporting events, diagnostics, metrics, and timing paths are typed public contracts. Keep human output on stderr, JSONL on stdout, preserve stable diagnostic codes, and update `docs/reporting.md` when their schema or semantics change.
- STA must never turn missing constraints or unsupported checks into a pass. Use `UNCONSTRAINED`, `PARTIALLY CONSTRAINED`, or `NOT ANALYZED`, and keep routed delay per sink rather than using the whole-net PIP union.

## Algorithm modernization status (2026-08-22, COMPLETE)

All four planned upgrades have landed on the algorithm-modernization branch:

1. Negotiated congestion routing (PathFinder-style): ResourceClaims track
   owner + foreign-claim counts per resource; searches allow temporary
   sharing priced by present factor x contention + accumulated history;
   passes repeat until conflict-free (32-pass cap, non-convergence is
   reported as a route note).
2. Timing-driven route costs: net criticality discounts site-local node
   costs by up to 25% during search. Net ORDER is deliberately untouched -
   low-fanout-first prevents local-escape starvation.
3. Real backward required-time propagation in STA: graph sinks are seeded
   with the worst arrival and requirements relax upstream, giving true
   per-node slacks that distinguish parallel branches.
4. Adaptive SA placement: Relocate moves join Swap with acceptance-driven
   weight rebalancing every 128 trials, plus gentle reheat when a 256-trial
   window closes below the acceptance floor.

Initial placement was already analytic (connectivity-weighted greedy toward
weighted centroids), so no change was needed there.

Note on oracles: after the VeriComm continuous-clock finding (below),
board validation must use settling-immune designs or Rust-vs-C++
differentials; cycle-exact goldens are not achievable in VeriComm mode.

### Incremental negotiated routing (2026-08-23, COMPLETE)

`ClaimIndex` now owns both resource→claimants and net→resources indexes.
Contended resources are derived from the resource claims instead of cached as
a third state. Negotiation rips up only nets touching those resources, keeps
every other route, and restores the original `net_order` before rerouting so
fixed seeds remain deterministic. Removing a net rebuilds contention from the
remaining typed claims, including legal synthetic-clock sharing.

Route reports include negotiation passes, final overuse, routed-net attempts,
and convergence. The dense benchmark generator uses a small fixed I/O surface
and a configurable internal register/LUT mesh; unlike the old `dense_20`
scratch generator, it does not measure conflicts caused by scores of
unconstrained I/O ports.

Release benchmark (`--width 192`, seed 1): 321 logic clusters and 839 routable
nets completed routing in about 3.1 s; pass 2 rerouted 8 nets and converged with
zero overuse. Blinky's routed XML and bitstream remain byte-identical to main.

## Board debug state (summary)

The full chronological debug log lives in `docs/board-debug-history.md`.
Latest entries there supersede earlier ones. Current conclusions:

- The VeriComm fixture runs a continuous free-running fabric clock, so
  cycle-exact absolute goldens are not achievable for history-dependent
  designs. Valid oracles are settling-immune designs (outputs converge to a
  pure function of current inputs) or board-vs-board Rust-vs-C++ differentials.
- `random_board_diff.py` goldens assume one clock edge per segment; re-check
  protocol assumptions before treating a "random case fails" report as a flow
  bug.
- The narrow-LUT / `lut_expr` / `physical_import` theory is dead. Do not resume
  it, and do not revisit `SYNCX`/`SYNCY` or safe-default slice-mux theories
  without new evidence.
- The checked-in `examples/board-e2e/manifest.json` entries are
  self-consistent hardware recordings, not RTL-semantics proofs. Only update
  them after freshly confirming the C++ baseline.

Working rules for board debugging:

- Filter first: freshly probe current C++ on the exact case and keep only
  cases where C++ is correct and Rust is wrong.
- The known-good current C++ chain needs `map -y` and
  `pack ... -g resources/hw_lib/fdp3_config.xml`.
- For Rust-only route bugs, check stitched-wire ownership early, not just
  exact route-node ownership.
- Keep changes small, preserve useful artifacts under `build/`, and record
  each tested theory (code delta, exact command, whether the board changed)
  in `docs/board-debug-history.md`.
