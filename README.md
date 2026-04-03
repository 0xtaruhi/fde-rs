# fde-rs

A standalone Rust implementation flow for FDE.

`fde-rs` is a Rust-first toolchain that consumes **EDIF** netlists, runs the full
implementation pipeline, and emits the same family of stage artifacts users
expect from FDE-style flows: mapped XML, packed XML, placed XML, routed XML,
STA reports, and deterministic bitstreams.

The project is intentionally **library-first, CLI-second**:

- reusable typed IR in Rust
- thin CLI orchestration
- deterministic outputs for a fixed seed
- stage-local logic instead of a giant monolith

## Highlights

- **End-to-end Rust flow**: `map -> pack -> place -> route -> sta -> bitgen`
- **Deterministic implementation**: same seed, same output
- **Board-oriented regressions** checked in under [`examples/board-e2e/`](examples/board-e2e)
- **Readable sidecar output** next to generated bitstreams for debugging
- **Yosys-friendly frontend**: synthesize to EDIF, then hand off to `fde-rs`

## Pipeline

| Stage | Input | Output |
| --- | --- | --- |
| `map` | EDIF | mapped XML |
| `pack` | mapped XML | packed XML |
| `place` | packed XML | placed XML |
| `route` | placed XML | routed XML with physical pips |
| `sta` | routed XML | timed XML + timing report |
| `bitgen` | routed/timed XML | deterministic `.bit` + sidecar |
| `impl` | EDIF + constraints | full staged run |

## Quick start

### 1) Build

```bash
cargo build
```

### 2) Run the full flow

```bash
cargo run --bin fde -- impl \
  --input examples/blinky/blinky.edf \
  --constraints examples/blinky/constraints.xml \
  --resource-root resources/hw_lib \
  --out-dir build/blinky-run
```

### 3) Inspect the outputs

Typical artifacts include:

- `01-mapped.xml`
- `02-packed.xml`
- `03-placed.xml`
- `04-routed.xml`
- `05-timed.xml`
- `05-timing.rpt`
- `06-output.bit`
- `06-output.bit.txt`
- `report.json`

## Command-line usage

Show top-level help:

```bash
cargo run --bin fde -- --help
```

Run individual stages:

```bash
cargo run --bin fde -- map --input design.edf --output build/01-mapped.xml
cargo run --bin fde -- pack --input build/01-mapped.xml --output build/02-packed.xml --family fdp3
cargo run --bin fde -- place --input build/02-packed.xml --output build/03-placed.xml \
  --arch resources/hw_lib/fdp3p7_arch.xml \
  --delay resources/hw_lib/fdp3p7_dly.xml \
  --constraints constraints.xml
cargo run --bin fde -- route --input build/03-placed.xml --output build/04-routed.xml \
  --arch resources/hw_lib/fdp3p7_arch.xml \
  --cil resources/hw_lib/fdp3p7_cil.xml \
  --constraints constraints.xml
cargo run --bin fde -- sta --input build/04-routed.xml --output build/05-timed.xml \
  --report build/05-timing.rpt \
  --arch resources/hw_lib/fdp3p7_arch.xml \
  --delay resources/hw_lib/fdp3p7_dly.xml
cargo run --bin fde -- bitgen --input build/04-routed.xml --output build/06-output.bit \
  --arch resources/hw_lib/fdp3p7_arch.xml \
  --cil resources/hw_lib/fdp3p7_cil.xml
```

## Frontend model: use Yosys first

This repository is **not** trying to be a full Verilog frontend.

The intended flow is:

1. synthesize with Yosys
2. write EDIF
3. run `fde-rs`

Bundled helper:

```bash
python3 scripts/synth_yosys_fde.py \
  --top your_top \
  --out-edf build/your_top.edf \
  path/to/your_top.v
```

If you already have your own Yosys flow, any compatible EDIF is fine.

## Board regressions

Checked-in board cases live under [`examples/board-e2e/`](examples/board-e2e).
Each case includes:

- a synthesized `.edf`
- a `constraints.xml`
- expected probe outputs recorded in [`manifest.json`](examples/board-e2e/manifest.json)

Run the live board regression suite:

```bash
python3 scripts/board_e2e.py run
```

Run one case:

```bash
python3 scripts/board_e2e.py run logic-mesh
```

Some cases use custom `probe_segments` in the manifest for longer stimulus
windows. This keeps hardware regressions reproducible from checked-in inputs.

## Development

### Fast local checks

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
```

### CI-parity command

```bash
cargo fmt --all -- --check && \
cargo check --locked --all-targets && \
cargo clippy --locked --all-targets --all-features -- -D warnings && \
cargo test --locked --quiet
```

### Useful scripts

- `python3 scripts/board_e2e.py run`
- `python3 scripts/random_board_diff.py --count 5 --seed 20260322 --keep-going`
- `python3 scripts/slice_config_diff.py --packed <02-packed.xml> --sidecar <06-output.bit.txt>`

## Repository layout

```text
src/
  app/        CLI and orchestration
  core/       typed IR and semantic domain helpers
  infra/      XML/EDIF/resource/constraint I/O
  stages/     map/pack/place/route/sta/bitgen logic
examples/     sample inputs and board regressions
docs/         design notes and refactor plans
scripts/      synthesis, board, and debug helpers
```

## Design principles

- **Determinism matters**
- **Typed IR first**
- **Thin CLI layer**
- **Clear stage boundaries**
- **Compatibility at the artifact boundary**

The public contract is the emitted XML/bitstream shape, not hidden Rust-only
intermediate formats.

## Status

`fde-rs` is under active development, but it already supports meaningful
end-to-end implementation, board-facing regressions, and bitstream generation in
Rust.

## License

This project is licensed under the terms of the [LICENSE](LICENSE) file.
