# Reporting and STA contract

This document defines the user-facing and machine-facing reporting contract for
`fde`. The terminal renderer is a view over typed events; it is not the source
of truth.

## Terminal modes

- Human mode writes progress and diagnostics to stderr. A normal successful
  flow emits one compact line per stage plus the final artifact paths.
- `-v` adds stage-level detail; `-q` keeps only diagnostics and the final
  result.
- `--color auto|always|never` and `--progress auto|always|never` are independent.
  `auto` only enables live rendering on a terminal. `NO_COLOR` and `TERM=dumb`
  disable automatic color.
- `--message-format json` writes one serialized `StageEvent` per stdout line.
  JSON mode never emits ANSI control characters or live-line rewrites.

The event discriminator is `event`. Stable event kinds are `flow_started`,
`started`, `progress`, `log`, `diagnostic`, `report`, `finished`, and
`flow_finished`. Progress includes a phase, current/total work,
typed work unit, percentage, and optional live metrics.

## Artifacts

| Artifact | Purpose |
| --- | --- |
| `summary.rpt` | Short outcome, inputs/resources, runtimes, QoR, and selected outputs |
| `run.log` | One chronological event trace followed by one summary; no duplicated stage dump |
| `report.json` | Complete versioned flow report, typed diagnostics, metrics, timing, artifacts |
| `05-timing.rpt` | Human timing summary, coverage, path groups, and detailed paths |
| `05-timing.json` | Structured `TimingSummary` for tools and CI |
| `04-routed.xml` | Routed design including serialized per-sink branches for standalone STA |

Failure after the output directory is created still writes a schema-valid
`report.json`, `summary.rpt`, and `run.log`. Completed stages retain their
metrics; the failing and remaining stages are marked failed/skipped as
appropriate; only artifacts that exist are advertised.

## Diagnostics

Diagnostics contain `code`, `severity`, `message`, and optional `detail`,
`help`, `object`, and `artifact`. Current stable codes include:

| Code | Meaning |
| --- | --- |
| `FDE-FLOW-0001` | Flow failed; a partial report was written |
| `FDE-ROUTE-0001` | Negotiated routing required the hard-blocking legalization pass |
| `FDE-ROUTE-0002` | No physical path was found for a sink |
| `FDE-ROUTE-0003` | Driver/sink or primitive route mapping is missing |
| `FDE-STA-0001` | No clock constraint; delay estimate only |
| `FDE-STA-0002` | One or more arcs use a fallback delay model |
| `FDE-STA-0003` | Timing violation promoted to failure by `--fail-on-timing` |
| `FDE-STA-0004` | Only part of the synchronous interface is constrained |

Renderers de-duplicate the same typed diagnostic when it appears both live and
inside the final stage report.

## Timing semantics

Every detailed path declares its startpoint, endpoint, check, path group,
launch/capture clocks, data arrival, data required time, slack, and logic level
count. Each point has an incremental and cumulative delay plus its source. The
sum of point increments reconciles with the reported path delay (including
clock-to-Q, input delay, and setup checks where applicable).

Routed nets retain a separate driver-to-sink branch. STA uses that branch's
delay and only falls back to the whole-net route for legacy artifacts without
per-sink data. Unrouted nets use the selected delay table or a clearly labeled
geometric estimate.

Timing status is deliberately conservative:

- `MET`: every register endpoint and every non-clock primary I/O endpoint is
  constrained, and all analyzed setup slacks are non-negative.
- `VIOLATED`: at least one analyzed setup slack is negative.
- `PARTIALLY CONSTRAINED`: clocks exist and analyzed paths pass, but synchronous
  register/I/O coverage is incomplete.
- `UNCONSTRAINED`: no clock constraint exists; Fmax is an estimate, not sign-off.
- `NOT ANALYZED`: the check is unsupported or was not run. Hold currently uses
  this status and is never silently treated as passing.

The strict SDC parser accepts one object per command for `create_clock`,
`set_input_delay`, `set_output_delay`, and setup `set_clock_uncertainty`.
Unsupported commands, unknown ports/clocks, duplicate constraints, invalid
directions, and non-finite/negative values are errors. Multiple clock domains
and cross-domain path labeling are supported; false paths, multicycle paths,
asynchronous clock groups, generated clocks, min/hold analysis, latches, and
block RAM timing are not yet modeled and therefore are rejected or reported as
not analyzed rather than guessed.

## Exit codes

- `0`: requested analysis/flow completed; inspect timing status when
  `--fail-on-timing` was not requested.
- `1`: input, resource, implementation, serialization, or unsupported-model
  failure.
- `5`: reports and normal artifacts were written, but setup timing is
  `VIOLATED` and `--fail-on-timing` was requested.
