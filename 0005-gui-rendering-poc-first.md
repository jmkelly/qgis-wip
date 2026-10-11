# GUI rendering POC before core stabilizes

ADR 0002 commits to iced+wgpu for the GUI, built last after headless core stabilizes. We are now building a rendering proof of concept (canvas + pan/zoom with 10k polygons) before the core crates are complete, because the most urgent technical risk is whether iced+wgpu can render a map canvas at interactive frame rates — if it cannot, the GUI toolkit decision in ADR 0002 needs to be revisited before we invest in porting 609 dialogs. The POC gives `geometry` and `crs` their first real implementations (`Point`, `Polygon`, equirectangular projection, `Camera`) and becomes the foundation for the full GUI port. ADR 0002's "built last" sequencing is superseded for the POC phase; it will be revisited after the POC answers the rendering performance question.

**Status**: accepted
**Supersedes**: ADR 0002 (sequencing only — iced+wgpu toolkit choice unchanged)

Note (ADR 0006): the `crs` implementation this POC produces is the render-surface `Camera` plus the
trivial degree-space `Equirectangular`, which is not a cartographic projection. Real CRS
reprojection is out of scope for the POC and arrives in E2 on top of `proj4rs`.
