# Board E2E

This directory contains board-probed EDF regression cases for `fde-rs`.

Each case lives in its own subdirectory with:

- one checked-in `.edf`
- one checked-in `constraints.xml`

Expected wave sequences are recorded in [`manifest.json`](/Users/zhangzhengyi/Documents/Projects/fde-rs-standalone/examples/board-e2e/manifest.json). Those values were refreshed from live board runs on 2026-03-21 using the current `wave_probe` flow.

Cases can optionally override the default probe waveform by setting
`probe_segments` in the manifest. This is used for long-cycle board regressions
such as `sticky16-check`, where the observable behavior only appears after a
longer repeated stimulus window.

Run the full suite with:

```bash
python3 scripts/board_e2e.py run
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
