# gpui → zgui spike: results (mint, 2026-11-27/28)

Machine: mint, 32 threads (`lscpu`), RTX 4090, Linux 6.17, X11 session on :0. Release builds, rustc 1.96.0 stable.
zgui = zortax/zgui @ ee29c7dc (git dep, builds on stable, no nightly needed). Medians of 5 runs per series, two series
(`runs-1.jsonl` quiet machine, `runs-2.jsonl` at load ~11-16); ranges in brackets.

Both sides render `surface250.json` (1 Box root, 1 heading Text, 62 cards × Box+Text+Row+Button = 250 flat A2UI
components) and a 10k-row List of Row(title, meta) at 390×800, scale factor 2.
- gpui: `eui_gpui::SurfaceView` (the real eui-core reducer + painter), headless frame = `layout_now` + `Window::draw` (no GPU submit). Code: `gpui/spike_gpui.rs` (run from `crates/eui-shot/examples/`).
- zgui: hand-written mapper of the same flat components to zgui elements, one `RwSignal` per bound string prop; list = `zgui_ui::virtualize::VirtualList` (fixed 40 px rows). Headless frame = `Harness::settle` with a renderer that drops the scene (no GPU submit). Code: `src/main.rs`.

| metric | gpui (eui-gpui) | zgui |
|---|---|---|
| 1. cold start → first frame, headless, process exec → first frame done (250 surface) | 64 ms [61-121] | 58 ms [50-102] (series 2: 78 [50-87]) |
| 1b. real X11 window: exec → first render()/view build call (not presented) | 329-371 ms | 236-264 ms |
| 2. scroll 10k List, 100 × one viewport, frame mean / p95 | 4.29 / 4.84 ms (s2: 4.11 / 4.31) | 1.56 / 2.39 ms (s2: 1.40 / 2.11) |
| 2b. 10k List first frame (data + build + layout) | 14 ms (context already up) | 32-36 ms (incl. runtime + font setup) |
| 3. ONE text-prop delta in the 250 surface → next frame done, mean / p95 of 200 | 9.56 / 10.3 ms (s2: 9.22 / 9.90) | 0.050 / 0.069 ms (s2: 0.038 / 0.052) |
| 4. idle CPU, real X11 window on :0, 10 s, nothing changing | 1.0-1.3 % of a core (113 threads) | 0.2-0.3 % (25 threads) |
| 5a. RSS after first frame (headless, 250 surface) | 60.4 MB | 35.6 MB |
| 5b. RSS list: after first frame → after 100 scroll steps (headless) | 64.1 → 71.1 MB | 28.5 → 37.9 MB |
| 5c. RSS real window, idle (GPU driver mapped) | 263 MB | 209-221 MB |
| 6. release binary (unstripped / stripped) | 113 / 97 MB (example; links gpui test-support + gpui-component w/ tree-sitter) | 49 / 38 MB |
| 6b. clean cold build of the spike (deps fetched, 413 crates, nice 19, 32 threads) | n/a (shared desktop workspace) | 54 s |

Notes
- The delta row is NOT like for like: gpui's number is the full product path (eui-core `update_components` → re-reduce → relayout + repaint of the whole surface); zgui's is a signal write → one text node reshaped. It shows what fine-grained reactivity buys, not that zgui's layout is 200× faster. A zgui port that keeps eui-core's reducer in front would land between the two.
- Scroll: both are pure CPU pipeline (no GPU). zgui moves a scroll space and builds only the arriving rows (one frame per step, offset verified at 80 000 css px after 100 steps).
- Text quality (shots/, 2×, both grayscale AA on Inter from `assets/fonts`): visually equivalent; zgui (Parley/swash + Vello) slightly bolder/softer stems, gpui (cosmic-text) a touch crisper. No hinting differences visible at 2×.
- Shots: `shots/gpui.png` (gpui wgpu offscreen) and `shots/zgui.png` (zgui-render-wgpu offscreen readback), 2560×2480, quantised to 256 colours to stay < 500 KB. Spacing differs slightly: the zgui CSS is hand-copied from the exponential dark theme, not generated from eui-core's tokens.
- Worked around: no programmatic scroll on the harness → `NodeRef::scroll_to(Offset)` (applied after the next layout: advance the clock + settle); `VirtualList` needs a declared fixed row height (no measured rows); the builder API needs `AnyView` erasure for a data-driven tree; a second document in one process is not supported (thread-local mount) → list and 250 benches are separate processes; `control` does not centre its text by default (needs `text-align`). No crash or panic. Headless/testkit (`zgui-platform-headless`, `scale_capture` example) made all of this straightforward; wgpu 30 + vello 0.11.
- API friction: `#[component]` props need the generated `*Props` type imported (`VirtualListProps`); docs are excellent (performance.md with banded counters).
- Idle/window runs put a window on the live :0 display for ~17 s each (no input sent); both self-exit.
- The parent branch `exp/session-9fd75797` did not exist on origin, so this branch is based on master 8901e10967.
