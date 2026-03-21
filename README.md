# fde-rs

Standalone Rust 2024 implementation flow for the FDE toolchain.

`fde-rs` is the Rust-first home for the modern FDE flow:

- main entrypoint: `fde`
- stage binaries: `map`, `pack`, `place`, `route`, `sta`, `bitgen`, `nlfiner`, `import`
- frontend assumption: synthesize with Yosys first, then hand EDIF to this project

## What Works

- `fde map`: EDIF -> mapped IR
- `fde pack`: mapped IR -> clustered IR
- `fde place`: clustered IR -> placed IR
- `fde route`: placed IR -> routed IR
- `fde sta`: routed IR -> timing summary + report
- `fde bitgen`: routed/timed IR -> deterministic `.bit` + readable sidecar
- `fde normalize`: cleanup/rename pass
- `fde impl`: one-command end-to-end implementation flow

The current `bitgen` is deterministic and regression-friendly. It emits CIL-backed tile config images for supported `SLICE`/`IOB`/`GCLK` sites in the sidecar and payload, and `fde impl` keeps the full implementation flow in Rust.

## Build

```bash
cargo build
```

## Tests

```bash
cargo test
```

## CI

GitHub Actions runs on every pull request, push to `main`, and manual dispatch.
The workflow currently checks:

- `cargo fmt --all -- --check`
- `cargo check --locked --all-targets`
- `cargo clippy --locked --all-targets --all-features -- -D warnings`
- `cargo test --locked --quiet`
- `cargo run --locked --quiet --bin fde -- impl ...` against the checked-in
  `examples/blinky` smoke design and `tests/fixtures/hw_lib`

You can run the same commands locally before opening a PR.

## Modern CLI

Show the top-level help:

```bash
cargo run --bin fde -- --help
```

Run the full flow:

```bash
cargo run --bin fde -- impl \
  --input examples/blinky/blinky.edf \
  --constraints examples/blinky/constraints.xml \
  --resource-root resources/hw_lib \
  --out-dir build/blinky-run
```

Dry-run the checked-in board-oriented EDF suite:

```bash
find examples/board-e2e -mindepth 2 -maxdepth 2 -name '*.edf' | sort | while read -r edf; do
  case_dir=$(dirname "${edf}")
  name=$(basename "${case_dir}")
  cargo run --bin fde -- impl \
    --input "${edf}" \
    --constraints "${case_dir}/constraints.xml" \
    --resource-root resources/hw_lib \
    --out-dir "build/board-e2e/${name}"
done
```

Run the live board suite in one shot:

```bash
python3 scripts/board_e2e.py run
```

The live board path uses the in-repo probe tool under [`tools/wave_probe/`](/Users/zhangzhengyi/Documents/Projects/fde-rs-standalone/tools/wave_probe), which depends on the published [`vlfd-rs` 1.0.0 crate](https://docs.rs/vlfd-rs/latest/vlfd_rs/). It does not require the legacy `FDE-Source` repository.

The default full resource bundle is vendored under [`resources/hw_lib/`](/Users/zhangzhengyi/Documents/Projects/fde-rs-standalone/resources/hw_lib).

Artifacts land in `build/blinky-run/`:

- `01-mapped.xml`
- `02-packed.xml`
- `03-placed.xml`
- `04-routed.xml`
- `04-device.json`
- `05-timed.xml`
- `05-timing.rpt`
- `06-output.bit`
- `06-output.bit.txt`
- `report.json`

Run individual stages:

```bash
cargo run --bin fde -- map \
  --input examples/blinky/blinky.edf \
  --output build/map.xml

cargo run --bin fde -- pack \
  --input build/map.xml \
  --output build/pack.xml \
  --family fdp3

cargo run --bin fde -- place \
  --input build/pack.xml \
  --output build/place.xml \
  --arch tests/fixtures/hw_lib/fdp3p7_arch.xml \
  --delay tests/fixtures/hw_lib/fdp3p7_dly.xml \
  --constraints examples/blinky/constraints.xml \
  --mode timing

cargo run --bin fde -- route \
  --input build/place.xml \
  --output build/route.xml \
  --arch tests/fixtures/hw_lib/fdp3p7_arch.xml \
  --constraints examples/blinky/constraints.xml \
  --mode timing

cargo run --bin fde -- sta \
  --input build/route.xml \
  --output build/sta.xml \
  --report build/sta.rpt \
  --arch tests/fixtures/hw_lib/fdp3p7_arch.xml \
  --delay tests/fixtures/hw_lib/fdp3p7_dly.xml

cargo run --bin fde -- bitgen \
  --input build/route.xml \
  --output build/out.bit \
  --arch tests/fixtures/hw_lib/fdp3p7_arch.xml \
  --cil tests/fixtures/hw_lib/fdp3p7_cil.xml
```

## External Resource Bundle Example

If you want to use an external hardware resource bundle, point `--resource-root` at its `hw_lib` directory:

```bash
cargo run --bin fde -- impl \
  --input examples/blinky/blinky.edf \
  --constraints examples/blinky/constraints.xml \
  --resource-root /path/to/hw_lib \
  --out-dir build/external-resource-run
```

The compatibility wrappers also accept compatibility invocations:

```bash
cargo run --bin map -- \
  -y \
  -i examples/blinky/blinky.edf \
  -o build/map.xml \
  -c /path/to/hw_lib/dc_cell.xml \
  -e

cargo run --bin pack -- \
  -c fdp3 \
  -n build/map.xml \
  -l /path/to/hw_lib/fdp3_cell.xml \
  -r /path/to/hw_lib/fdp3_dcplib.xml \
  -o build/pack.xml \
  -g /path/to/hw_lib/fdp3_config.xml \
  -e
```

## Yosys Frontend

This repo does not try to be a full Verilog parser. Use Yosys first, for example:

```bash
yosys -p 'read_verilog your_top.v; synth -top your_top; write_edif synth.edf'
```

Then feed `synth.edf` into `fde map` or `fde impl`.

## Repository Scope

- This repository only contains the Rust implementation flow.
- Legacy C++ sources live elsewhere and are not part of `fde-rs`.
- Resource XML compatibility is preserved at the file-format boundary, not by co-locating the old monolith.
- Board-oriented example netlists live under [`examples/board-e2e/`](/Users/zhangzhengyi/Documents/Projects/fde-rs-standalone/examples/board-e2e) as checked-in EDF artifacts with per-case constraints and a board-probed manifest.
