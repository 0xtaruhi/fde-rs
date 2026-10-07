# Rust Refactor Plan

This document records the current refactor direction for the Rust rewrite.

## Goals

- Keep the implementation flow pure Rust and library-first.
- Move business logic away from ad-hoc string matching and into typed semantics.
- Split broad stage modules into smaller domain-focused modules.
- Preserve determinism and end-to-end functionality during refactor.
- Improve ergonomics: fewer lookups by raw names, friendlier errors, clearer APIs.

## Architectural Direction

- `app/cli/` remains a thin adapter layer.
- `app/` owns orchestration, reporting, and stage composition.
- `core/domain/` owns typed semantic concepts; `core/ir/` owns the shared IR.
- `infra/` owns parsing and persistence for EDIF, XML, JSON, CIL, constraints,
  and hardware resources.
- `stages/` owns map/pack/place/route/sta/bitgen algorithms.

## Current Top-Level Layout

The `src/` root should stay compact. Top-level directories are grouped as:

- `src/app`: CLI, orchestration, reporting
- `src/core`: domain semantics and IR
- `src/infra`: parsers, persistence, resource loading
- `src/stages`: implementation stages and stage-local helpers
- `src/main.rs`: the `fde` executable entrypoint (thin wrapper over `app::cli`)

Bitgen-related support code should be kept together under the `bitgen` subtree instead of spreading
device lowering, config image building, route-bit derivation, and frame serialization across many
separate stage roots.

## String Usage Policy

Strings are allowed at the boundaries:

- external resource names from EDIF/XML/CIL
- user-visible labels and reports
- file paths and artifact names

Strings should not drive core logic directly in stage code. Instead:

- parse endpoint kinds into enums
- classify primitive kinds into enums
- classify device site kinds into enums
- classify synthetic net origins into enums
- centralize any unavoidable name normalization inside semantic helper modules

## Planned Phases

Status as of 2026-10: Phases 1-3 have largely landed (`core/domain/`, the
split `core/ir/` with typed IDs and `DesignIndex`, and classified routing
metadata). Phase 4 is partially done; Phase 5 is open.

### Phase 1: Semantic Cleanup (largely done)

- Add typed semantic enums for endpoint kind, primitive kind, site kind, and net origin.
- Expose helper methods on IR and device types so stage code can avoid raw string branching.
- Refactor the highest-value hotspots first:
  - `route/mapping/mod.rs`
  - `sta/mod.rs`
  - `analysis/criticality.rs`
  - `place/model.rs`

### Phase 2: IR Decomposition (done)

- Split `ir/mod.rs` into smaller modules:
  - design
  - port
  - cell
  - net
  - cluster
  - endpoint
  - timing
- Introduce typed IDs for cells, nets, ports, and clusters.
- Add lookup helpers so stage code stops doing repeated linear scans.

### Phase 3: Device and Architecture Semantics (largely done)

- Separate raw parsed XML data from classified architecture views.
- Replace stringly device fields in the core with typed semantic wrappers where practical.
- Build reusable classified views for site kinds, tile classes, and routing resources.

### Phase 4: Stage Decomposition (in progress)

- Split large modules into service-oriented submodules:
  - `place`: init, improve, legalize, incremental
  - `route`: physical router application, device lowering handoff, pip materialization
  - `sta`: graph, delay, propagate, report
  - `bitgen`: lowering, device routing, config image, emit
- Remaining large files: `place/solver.rs`, `place/cost.rs`, `route/router.rs`.

### Phase 5: Error Model and Contracts (open)

- Move stage APIs toward typed error enums.
- Keep `anyhow` at the application boundary, not as the only internal contract.
- Add more structural tests:
  - semantic classifier tests
  - determinism regression tests
  - consistency checks from routed design to config image and bitstream

## Current Slice

- Finish removing remaining raw string branches from stage code (for example,
  IR metadata such as the design source format is now a typed enum).
- Split the remaining large place/route modules.
- Introduce typed stage errors at public stage boundaries (Phase 5).
