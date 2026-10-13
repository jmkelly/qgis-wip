# Feature map: qgis-bench

## What it does

Criterion benchmarks porting qgsbench hot paths. Four bench groups:
render (polygon_fill, layer_render, labeling), index (rtree_insert,
rtree_query), expression (parse, evaluate), raster (reproject, render).
All call placeholder stubs at E0 — real algorithms come in E1+.

## How to verify

```bash
./verify bench              # compile only (CI gate)
./verify bench --run        # run benchmarks (slower)
```

## What proves each feature works

| Feature | Evidence |
|---------|----------|
| Benchmarks compile | `./verify bench` (CI gate) |
| Benchmarks run without panic | `./verify bench --run` |
| No regression vs baseline | Compare `./verify bench --run` output |
| Crate is `#![forbid(unsafe_code)]` | Enforced by crate-level attribute |

## Benchmark vetting

Before reporting any number from `./verify bench --run`:

1. **What limits the number?** Why isn't it double?
2. **Was every side tuned the same?** Release mode? Same input?
3. **Does it break a physical limit?** (disk bandwidth, core count)
4. **Did anything error or return wrong output?**
5. **Does it reproduce?** Run alternating, take median + range.
6. **Does it matter end-to-end?** On the path a user waits on?
7. **Did the work happen inside the timed region?**

Verdict: faster, slower, no measurable difference, or inconclusive.
Say "inconclusive" when you can't name the limiter or a side ran untuned.

## Groups

| Group | Benchmarks |
|-------|-----------|
| render | polygon_fill, layer_render, labeling |
| index | rtree_insert, rtree_query |
| expression | parse, evaluate |
| raster | reproject, render |
