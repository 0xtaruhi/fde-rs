# AGENTS Guide for fde-rs

This repository is the standalone Rust 2024 implementation flow for FDE.

## Repository Facts

- Primary product shape: Rust library first, CLI second, compatibility wrappers last.
- Primary executable: `fde`.
- Compatibility executables: `map`, `pack`, `place`, `route`, `sta`, `bitgen`, `nlfiner`, `import`.
- Primary frontend assumption: Yosys produces EDIF; this repo consumes EDIF and downstream IR.
- This repo is independent from the legacy C++ monolith. Do not reintroduce old mixed-repository assumptions or a single giant pipeline module.
- Determinism matters: fixed seeds should give reproducible output, even if internal work is parallelized.

## Architectural Direction

- Shared typed IR lives in Rust and is reused across all stages.
- Stage logic belongs in focused modules like `map`, `pack`, `place`, `route`, `sta`, `bitgen`, `normalize`, `orchestrator`.
- CLI code should stay thin: argument parsing, file orchestration, report writing, progress output.
- Compatibility wrappers should translate old flags into the new library API; they are not the main product surface.
- Follow the refactor plan in `docs/refactor-plan.md` when reshaping modules.
- Keep `src/` top-level compact by grouping modules under `app/`, `core/`, `infra/`, and `stages/` instead of adding more root directories.

## Scope Boundaries

- Verilog import is intentionally minimal. Prefer failing clearly and telling the user to run Yosys.
- `bitgen` materializes CIL-backed site SRAM images for supported logic/IO/clock sites and stays within the Rust implementation flow.
- Reference hardware XML compatibility matters. Reuse established FDE hardware XML conventions and invocation shapes where practical.

## Commands

- Build: `cargo build`
- Check: `cargo check`
- Test: `cargo test`
- CI parity: `cargo fmt --all -- --check && cargo check --locked --all-targets && cargo clippy --locked --all-targets --all-features -- -D warnings && cargo test --locked --quiet`
- CI smoke: `cargo run --locked --quiet --bin fde -- impl --input examples/blinky/blinky.edf --constraints examples/blinky/constraints.xml --resource-root resources/hw_lib --out-dir /tmp/fde-rs-ci-smoke`
- Board EDF dry run: `find examples/board-e2e -mindepth 2 -maxdepth 2 -name '*.edf' | sort | while read -r edf; do case_dir=$(dirname "${edf}"); name=$(basename "${case_dir}"); cargo run --bin fde -- impl --input "${edf}" --constraints "${case_dir}/constraints.xml" --resource-root resources/hw_lib --out-dir "build/board-e2e/${name}"; done`
- Live board run: `python3 scripts/board_e2e.py run`
- In-repo board probe: `cargo run --manifest-path tools/wave_probe/Cargo.toml -- <bitstream>`
- Main help: `cargo run --bin fde -- --help`
- End-to-end smoke: `cargo run --bin fde -- impl --input examples/blinky/blinky.edf --constraints examples/blinky/constraints.xml --resource-root tests/fixtures/hw_lib --out-dir build/blinky-run`

## Editing Guidance

- Keep ASCII unless the file already requires something else.
- Prefer small stage-focused modules over broad refactors that blur responsibilities.
- Do not silently swallow missing resource/config inputs; either derive a safe default or surface a clear error.
- When adding new tooling, update this file and `README.md` in the same change.
- Keep checked-in board regression netlists in EDF form under `examples/board-e2e/`; do not commit temporary synthesis-only Verilog there.
- Keep string handling at parsing and reporting boundaries; do not add new raw string branching in core stage logic when a typed enum or helper can model it.
- Prefer semantic helper modules in `domain/` over repeating `eq_ignore_ascii_case`, `to_ascii_lowercase`, or string literal matches across stage code.
