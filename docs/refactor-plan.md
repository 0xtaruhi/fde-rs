# Rust Refactor Plan

This document records the current refactor direction for the Rust rewrite.

## Goals

- Keep the implementation flow pure Rust and library-first.
- Move business logic away from ad-hoc string matching and into typed semantics.
- Split broad stage modules into smaller domain-focused modules.
- Preserve determinism and end-to-end functionality during refactor.
- Improve ergonomics: fewer lookups by raw names, friendlier errors, clearer APIs.

## Architectural Direction

- `cli/` remains a thin adapter layer.
- `app/` owns orchestration, reporting, and stage composition.
- `domain/` owns typed semantic concepts and durable core types.
- `adapters/` owns parsing and persistence for EDIF, XML, JSON, and CIL.
- `stages/` owns map/pack/place/route/sta/bitgen algorithms.

## Current Top-Level Layout

The `src/` root should stay compact. Top-level directories are grouped as:

- `src/app`: CLI, orchestration, reporting
- `src/core`: domain semantics and IR
- `src/infra`: parsers, persistence, resource loading
- `src/stages`: implementation stages and stage-local helpers
- `src/bin`: the primary executable entrypoint(s)

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

### Phase 1: Semantic Cleanup

- Add typed semantic enums for endpoint kind, primitive kind, site kind, and net origin.
- Expose helper methods on IR and device types so stage code can avoid raw string branching.
- Refactor the highest-value hotspots first:
  - `route/mapping/mod.rs`
  - `sta/mod.rs`
  - `analysis/criticality.rs`
  - `place/model.rs`

### Phase 2: IR Decomposition

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

### Phase 3: Device and Architecture Semantics

- Separate raw parsed XML data from classified architecture views.
- Replace stringly device fields in the core with typed semantic wrappers where practical.
- Build reusable classified views for site kinds, tile classes, and routing resources.

### Phase 4: Stage Decomposition

- Split large modules into service-oriented submodules:
  - `place`: init, improve, legalize, incremental
  - `route`: physical router application, device lowering handoff, pip materialization
  - `sta`: graph, delay, propagate, report
  - `bitgen`: lowering, device routing, config image, emit

### Phase 5: Error Model and Contracts

- Move stage APIs toward typed error enums.
- Keep `anyhow` at the application boundary, not as the only internal contract.
- Add more structural tests:
  - semantic classifier tests
  - determinism regression tests
  - consistency checks from routed design to config image and bitstream

## Current Slice

The active slice is Phase 1:

- establish `domain/` semantic modules
- refactor endpoint/site/origin/primitive classification out of raw string branches
- keep I/O schemas stable while reducing string-driven control flow
