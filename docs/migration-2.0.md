# Migrating from fde 1.x to 2.0

Version 2.0 preserves the stage runner entry points, but changes public Rust
data types. Consumers must explicitly update their Cargo dependency to `fde =
"2.0.0"`.

## Rust API

- `ir::Metadata::source_format` is now `ir::SourceFormat`, rather than `String`.
  Replace assignments such as `"edif".to_string()` with `SourceFormat::Edif`.
  The historical JSON spellings remain accepted, including `"EDIF"`.
- `CellTimingModel` now includes `block_ram: Option<BlockRamTiming>`.
  Prefer `load_cell_timing_model` for a library-backed model, or add
  `..CellTimingModel::default()` to existing struct literals. `None` means block
  RAM paths cannot be signed off.
- `TimingCoverage` has additional public fields for block RAM capture and
  launch coverage. Add `..TimingCoverage::default()` to struct literals that
  need only some fields. New JSON fields default to zero when reading older
  reports.

## Timing behavior

Block RAM is a synchronous boundary with one clock domain per connected port.
Inputs capture on that port's clock and outputs launch from it. Dual-port
mapping preserves distinct `CLKA` and `CLKB` pins.

Missing block RAM clock-to-out timing prevents a `MET` result, including ports
that only launch paths. Such paths use an optimistic zero launch delay to
expose violations, emit `FDE-STA-0005`, and require library timing before
sign-off. Hold analysis remains `NOT ANALYZED`.

The human-readable timing report now includes data paths, delay breakdowns,
and timing calculations. Consume typed JSON or `StageReport` metrics for
automation. A missing `timing_met` metric is not evidence of timing closure.
