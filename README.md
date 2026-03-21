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
  --resource-root tests/fixtures/hw_lib \
  --out-dir build/blinky-run
```

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
