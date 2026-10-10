# FDE 2.0.1

Full-width dual-port BRAM designs now route successfully. The router assigns
saturated BRAM data-input/output channels together before path search and
preserves their ownership through congestion negotiation and legalization.
This fixes the 256x16 direct-I/O and autonomous self-test designs that failed
in earlier releases. Public APIs and hardware-library formats are unchanged.

Validation includes both designs with placement seeds 1 and 2, zero final
resource overuse, and byte-identical 2-bit/8-bit outputs relative to the
board-validated 2.0.0 baseline. Live hardware checks pass for all three distinct
16-bit images using the shared continuous P77 clock: 768 direct dual-port
readback stages and full-depth autonomous self-tests with done/pass/fail 1/1/0.
Commands, raw-artifact locations, and scope are recorded in
`docs/board-debug-history.md`.

The lockfile updates `chacha20` from the yanked 0.10.1 version to 0.10.2,
which fixes SSE4.1 intrinsic use in its SSE2 RNG backend. See the
[upstream changelog](https://github.com/RustCrypto/stream-ciphers/blob/master/chacha20/CHANGELOG.md).
