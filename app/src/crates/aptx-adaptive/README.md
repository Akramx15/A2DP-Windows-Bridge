# aptx-adaptive (vendored)

The pure-Rust aptX Adaptive encoder (R1 / R2 bitstreams, HQ and low latency,
44.1 / 48 / 96 kHz) of [AKS-Logic/aptx-adaptive-linux](https://github.com/AKS-Logic/aptx-adaptive-linux),
`crates/aptx-adaptive` at commit `25099004b2d33ff86a661a85f3c8acb5d7b7e177`
(2026-10-22), MIT licensed (`LICENSE`). See `LEGAL.md` for the upstream
project's statement on how the codec and its numeric tables were obtained.

Changes from upstream:

- `src/codec/tables.rs` includes `bundled_tables.rs` directly instead of the
  `build.rs` output. Upstream's `build.rs` writes the same file unless
  `APTX_ADAPTIVE_SO` names a vendor library to re-extract the tables from;
  that path (and its `sha2` build dependency) is left out.
- `Cargo.toml` stands alone (upstream's workspace settings and lints inlined).
- The integration tests, examples and the 12 MB of reference vectors are not
  copied. The crate is excluded from the A2DPWB workspace, so the unit tests
  that read those vectors are not run here; upstream's full suite (98 tests,
  bit-exact against the vectors) passes on Windows x64 with Rust 1.99.

A2DPWB's own tests of the codec integration are in `app/src/crates/codec`.
