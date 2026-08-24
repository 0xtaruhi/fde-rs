# Board E2E

This directory contains board-probed EDF regression cases for `fde-rs`.

Each case lives in its own subdirectory with:

- one checked-in `.edf`
- one checked-in `constraints.xml`

Expected wave sequences are recorded in [`manifest.json`](manifest.json). Legacy
values were refreshed from live board runs on 2026-03-21 using the current
`wave_probe` flow.

The twelve `comb-*` cases are settling-immune, RTL-backed regressions. They
exhaust all sixteen values on VeriComm inputs `P151`, `P148`, `P150`, and
`P152`, checking outputs `P7`, `P6`, `P5`, and `P4`. Their 192 expected
vectors come from Icarus simulation and were matched exactly on a live board
on 2026-08-24. The suite covers Boolean logic, muxes, decoding, comparison,
addition, subtraction, rotation, priority encoding, population thresholds,
Gray encoding, barrel shifting, and a 4-bit S-box.

The ten `seq-*` cases exercise real sequential cells: synchronous and
asynchronous reset, register capture, clock enable, pipelines, up/down
counters, feedback convergence, an FSM, a bounded accumulator, and one-hot
state. Every input alternates a reset segment with a run segment. Because
VeriComm supplies a continuous fabric clock, both segments are sampled only
after the design reaches a fixed point. This gives a deterministic RTL oracle
without pretending that asynchronous board samples are cycle-exact. All 160
simulation observations matched the live board on 2026-08-24.

Cases can optionally override the default probe waveform by setting
`probe_segments` in the manifest. This is used for long-cycle board regressions
such as `sticky16-check`, where the observable behavior only appears after a
longer repeated stimulus window.

Run the full suite with:

```bash
python3 scripts/board_e2e.py run
```

Run the reproducible RTL simulations without hardware:

```bash
python3 scripts/board_e2e.py simulate
```

Run only the RTL-backed cases on a connected board:

```bash
python3 scripts/board_e2e.py run --rtl-only
```

List the cases with:

```bash
python3 scripts/board_e2e.py list
```

For cases that already have proven hardware bitstreams under a sibling `FDE-Source/build/hw-io-probe/`, compare the current flow against that baseline with:

```bash
python3 scripts/board_diff.py run
```

`board_diff.py` probes every discoverable baseline bitstream and only compares
against the baseline when those candidates agree on one output sequence. It
does not use the manifest `expected_outputs` values to pick the baseline.
