# Board E2E

This directory contains board-probed EDF regression cases for `fde-rs`.

Each case lives in its own subdirectory with:

- one checked-in `.edf`
- one checked-in `constraints.xml`

Expected wave sequences are recorded in [`manifest.json`](/Users/zhangzhengyi/Documents/Projects/fde-rs-standalone/examples/board-e2e/manifest.json). Those values were refreshed from live board runs on 2026-03-21 using the current `wave_probe` flow.

Run the full suite with:

```bash
python3 scripts/board_e2e.py run
```

List the cases with:

```bash
python3 scripts/board_e2e.py list
```
