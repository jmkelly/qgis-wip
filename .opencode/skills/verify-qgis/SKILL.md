---
name: verify-qgis
description: Project verification skill for the QGIS Rust workspace. Use when asked to "verify", "prove", "show evidence", or "check" that a change works. Replaces ad-hoc shell commands with a single composable CLI.
agent: opencode
---

# verify-qgis — Project Verification Skill

This skill proves behavior in the QGIS Rust workspace. It replaces ad-hoc
shell chains with a single composable CLI so every agent drives the project
the same way.

## When to use

Use this skill when:

- A change is done and you need to **prove it works**
- You're asked to **verify**, **validate**, or **show evidence**
- You need to know **what tests cover a crate**
- You want to run the **oracle diff** (Rust vs C++ or Rust vs golden)
- You need to **vet a benchmark number** before reporting it

## How to drive the workspace

All verification goes through `./verify` — never chain `cargo` commands
ad-hoc in a prompt.

```bash
./verify test                      # full workspace tests
./verify test --crate geometry     # geometry + its dependents
./verify clippy                    # clippy -D warnings
./verify clippy --crate crs        # scoped clippy
./verify oracle                    # verify all goldens
./verify oracle --fixture path     # verify one fixture
./verify diff <fixture>            # rust vs golden
./verify diff <fixture> --cpp      # rust vs C++ (needs QGIS_CPP_BINARY)
./verify bench                     # compile benchmarks
./verify bench --run               # run benchmarks
./verify doctor                    # environment health check
./verify all                       # everything (like ci.sh)
./verify agent --list              # list agent-runnable validation suites (E16)
./verify agent <suite>             # run one suite; alias ./verify agent-<suite>
```

`./verify agent <suite>` runs the `qgis-validate` harness and prints a
human-readable report plus a machine-readable JSON block, exiting `0` iff no
check failed. Suites are registered in `validate/src/suites/mod.rs`; a suite
check reports `pass`, `fail`, or `skip` (a skipped check — e.g. a database that
is not running — does not fail the run).

## Feature map

Each crate's features and their evidence live in `features/`:

| File | Crate | What it proves |
|------|-------|----------------|
| [features/geometry.md](features/geometry.md) | qgis-geometry | Geometry primitives, WKT/WKB, validity |
| [features/crs.md](features/crs.md) | qgis-crs | CRS parsing, WKT2, projections |
| [features/expression.md](features/expression.md) | qgis-expression | Expression parse + evaluate |
| [features/providers.md](features/providers.md) | qgis-providers | Data provider facade |
| [features/oracle.md](features/oracle.md) | qgis-oracle | C++/Rust diff harness |
| [features/bench.md](features/bench.md) | qgis-bench | Performance regressions |

Read the relevant feature file before verifying a change in that crate. It
lists the exact tests and oracle fixtures that prove behavior.

## Evidence rules

See [`AGENTS.md`](../../../AGENTS.md) § "Evidence rules" for the full evidence
rules. Summary:

1. "It compiles" is not evidence — run the gate and show output.
2. Match the check to the change.
3. Show artifacts — an artifact beats a plausible explanation.
4. If a check can't run, say "inconclusive".
5. A confident reply without evidence is a red flag.

## Benchmark vetting

Before reporting any performance number, run through this checklist:

1. **What limits the number?** Why isn't it double?
2. **Was every side tuned the same?** Release mode? Same input?
3. **Does it break a physical limit?** (disk bandwidth, core count)
4. **Did anything error or return wrong output?**
5. **Does it reproduce?** Run alternating, take median + range.
6. **Does it matter end-to-end?** On the path a user waits on?
7. **Did the work happen inside the timed region?**

Verdict: faster, slower, no measurable difference, or inconclusive.

## Maintenance

Run `./verify all` before declaring work done. Run `./verify doctor` to
check environment health. If a feature map entry is stale, fix it — the
map is infrastructure, not a one-off.

Treat this skill as committed infrastructure. If agents keep writing
throwaway scripts, add a subcommand to `./verify` instead.
