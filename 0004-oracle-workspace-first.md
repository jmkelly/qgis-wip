# Oracle harness + small-crate workspace before porting

Agents cannot converge on full-suite parity without a fast loop; we decided to build the C++/Rust diff oracle, `criterion` benches from `tests/bench/qgsbench`, and small workspace crates first in order core-geometry/expression/crs → providers → analysis → server → gui, because porting without that scaffolding guarantees guessing and slow verification.

Within that order, `crs` means the CRS facade over `proj4rs` plus our gap builds — see [ADR 0006](0006-proj4rs-projections.md). The projection math is not rebuilt from scratch.
