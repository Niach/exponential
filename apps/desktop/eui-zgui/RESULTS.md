# #EDE-279 zgui trial: eui-zgui vs eui-gpui — Linux lane (mint, 2026-11-28)

Machine: mint, 32 threads, RTX 4090, Linux 6.17, X11 on :0, release builds, rustc 1.96.0.
zgui = zortax/zgui @ `ee29c7dc` (pinned; still upstream HEAD on 2026-11-28, no newer rev needed).
Both renderers go through **eui-core** (same reducer, theme, taffy layout, windowed List, pixel snap at 2×):
gpui = `eui_gpui::SurfaceView` (`gpui/zgui_ref.rs`, run from `crates/eui-shot/examples/`, own target);
zgui = this crate (`Doc`: eui-core `Surface` + `ZMeasure` + the element mirror). Commands: [README.md](README.md).
Raw runs: `runs/*.jsonl`; medians of 5, ranges in brackets.

## Architecture (as asked: eui-core = engine, eui-zgui = painter + measurer + host)

- **Measurer** (`src/measure.rs`): eui-core's `Measure` answered with zgui's own parley shaper
  (`zgui_text::lay_out` over `zgui_text_parley::Shaper` from the app's `Fonts`), so the core reserves exactly what zgui paints.
  Control sizes mirror eui-gpui's `measure_leaf` for the trial's components.
- **Painter** (`src/doc.rs`, `src/paint.rs`): every placed node = one absolutely positioned zgui element at the core's frame;
  its inline style is the resolved `Visual` (fill, border, radii, shadow, opacity, clip, ink); leaves add text / vector
  (Lucide SVG from the shared registry) / box pieces. Per-slot signals; a pass writes only what changed
  (one-prop delta = 1 node restyled). Scroll containers move one inner element (`transform`).
- **Not a hand-built `zgui_scene::Scene`**: blocked for text. Glyphs reach the scene only through zgui-paint's document walk
  (atlas tiles → `MonoSprite`s, `crates/zgui-paint/src/emit/text.rs`); a scene built outside it has quads and vector items
  but no glyph path (`zgui-canvas` has no text either). The retained element mirror is the lowest seam that paints text;
  zgui's own layout only places absolute boxes at eui-core's frames.

## 1. Desktop conformance shots (Linux) — threshold fixed before the first run: ≤ 1.0 % pixels with any channel Δ > 8

720 px wide × the gpui content height, scale 2, theme `exponential` dark, the specimen's forced states.
`compare dirs` → `shots/linux/table.md` (+ `results.json`); side-by-sides (gpui | zgui | diff) in `shots/linux/*.side.webp`.

| component | differing % | pass | % beyond a 1 px shift (info) | what differs |
|---|---|---|---|---|
| badge | 0.87 | yes | 0.59 | glyph AA only |
| button | 0.96 | yes | 0.79 | glyph AA only |
| card | 0.67 | yes | 0.57 | glyph AA only |
| checkbox | 0.73 | yes | 0.53 | glyph AA only |
| grid | 0.66 | yes | 0.60 | glyph AA only |
| heading | 0.71 | yes | 0.55 | glyph AA only |
| icon | 0.54 | yes | 0.37 | glyph AA only |
| list | 0.71 | yes | 0.56 | glyph AA only |
| switch | 0.86 | yes | 0.67 | glyph AA only |
| textarea | 0.74 | yes | 0.63 | glyph AA only |
| progress | 1.05 | no | 0.66 | AA + bar end rounding |
| row | 1.19 | no | 1.00 | AA, trailing slot glyphs |
| stack | 1.30 | no | 1.13 | AA |
| tabs | 2.18 | no | 1.91 | strip close/add glyph sizes, list height ±1 px |
| input | 2.42 | no | 1.29 | adornments (search/clear glyphs) not ported |
| markdown | 2.81 | no | 2.61 | my Markdown is a subset (no links, list indent, code chip) — eui-gpui's is ~4k lines |
| text | 3.68 | no | 3.32 | drop cap, middle truncation, search highlights not ported |
| select | 4.39 | no | 3.50 | pill font/glyph sizes, loading spinner, row label part |
| code-block | 7.99 | no | 7.45 | **font matching**: keyword weight 600 → zgui picks JetBrains Mono **Bold** (CSS nearest-weight), gpui draws Regular |
| dialog | n/a | — | — | the gpui HEADLESS reference paints no in-surface layer (only the trigger); zgui paints the dialog + scrim and matches the committed app shot `shots/exponential-ui-dialog/desktop.webp` by eye (`shots/linux/dialog.zgui.webp`) |

**10 / 19 measurable pass; every pass is anti-aliasing-only** (swash/Vello vs cosmic-text rasterisation, ~0.5–1 % of pixels
on text-dense shots: that is the floor of a 1 % gate between two text stacks). All failures are unported painter details
(this session's port covers the common path of the 20 components, not eui-gpui's ~20k painter lines), except one real
renderer divergence: font-weight matching (code-block). **Verdict Linux: not yet (partial)** — no zgui blocker found, but the
gate is not met; macOS half pending (parent).

## 2. Variable row heights (chat-style), 10k rows, scroll 100 viewports

List of `Box(padding 12, radius 12, muted bg, marginBottom 8) > Text` with 1–8 wrapped lines at 390 px; eui-core windows it
(measured heights) on both renderers; both reach 80 000 px. A frame = core pass + the renderer's frame, no GPU submit.

| series | gpui mean / p95 / max | zgui mean / p95 / max |
|---|---|---|
| jump 1 viewport / frame, quiet machine (load ~3) | 2.44 / 2.75 / 3.18 ms | 2.87 / 3.25 / 3.59 ms |
| jump 1 viewport / frame, interleaved (load 13–190) | 3.17 / 3.88 / 5.72 ms | 3.97 / 4.82 / 6.75 ms |
| smooth 40 px / frame (2000 frames), interleaved, loaded | 1.51 / 3.19 / 15.8 ms | 1.64 / 4.23 / 23.3 ms |
| first frame (10k data + layout) | 18.3 ms | 19.1 ms |

zgui split (quiet): core layout 0.78 ms + mirror 0.3 ms + zgui frame ~1.8 ms; ~27 new elements per jump.
**Verdict: go** — variable heights work through eui-core's windowed List, within ~20 % of gpui, far under a 16.7 ms frame.

Fixed-height rows (the spike's 10k `Row(title, meta)` list) are zgui's weak case:

| series | gpui mean / p95 | zgui mean / p95 |
|---|---|---|
| jump 1 viewport, quiet | 4.21 / 4.49 ms | 7.49 / 8.65 ms |
| jump 1 viewport, interleaved loaded | 6.54 / 10.1 ms | 13.2 / 19.0 ms |
| smooth 40 px, interleaved loaded | 7.58 / 13.9 ms | 9.45 / 16.3 ms |

A jump brings ~124 new elements; zgui builds + restyles (inline style text through stylo) each, ~60–70 µs apiece, where
gpui (immediate mode) has no element cost. Reusing elements for reused slots (`EUI_ZGUI_RECYCLE`, default on) did not help:
the core frees and re-adds slots across parents, so the keyed `For` recreates them (13.2 vs 13.6 ms). The spike's 1.56 ms was
zgui's own fixed-row `VirtualList`, not eui-core's window.

## 3. Multiple surfaces per process — **go**

`tests/two_surfaces.rs` (`cargo test --release --test two_surfaces`, passes): two `Doc`s (two eui-core `Surface`s) in one
zgui window, real wgpu offscreen frames read back; updating A repaints A and leaves B's pixels byte-identical, and vice versa.
`zbench two 15` mounts both in one real window on :0 (static, 25 threads, 271 MB; the independence proof is the test). The spike's "one mount per thread" limit does not apply: a `Doc` is plain
data and any number mount as subtrees of one zgui document (zgui's `use_windows()` also opens more windows; not needed here).

## 4. Spike harness metrics, both through eui-core (medians of 5)

| metric | eui-gpui | eui-zgui |
|---|---|---|
| cold start → first frame, headless, exec → frame done (250 surface), quiet | 60.4 ms [59.0-74.3] | 68.9 ms [63.1-91.7] |
| same, interleaved loaded | 86.6 [76.7-177] | 84.2 [75.3-120] |
| real window: exec → first view build | n/a (not printed) | 260 ms [240-407] |
| one-prop delta → next frame, mean / p95 / max, quiet | 9.42 / 10.2 / 13.2 ms | **3.50 / 3.67 / 4.17 ms** |
| same, interleaved loaded | 14.2 / 17.7 / 48.5 | 5.22 / 7.23 / 11.2 |
| 10k fixed-row scroll, 100 viewports (mean / p95), quiet | **4.21 / 4.49** | 7.49 / 8.65 |
| 10k variable-row scroll (see 2), quiet | 2.44 / 2.75 | 2.87 / 3.25 |
| idle CPU, real window on :0, 10 s, no input (5 runs) | 0.8 % [0.7-1.0], 113 threads | **0.4 % [0.2-0.4], 25 threads** |
| RSS after first frame, headless (250 surface) | 61.1 MB | 60.3 MB |
| RSS list: first frame → after 100 jumps (fixed) | 64.0 → 70.9 MB | 53.5 → 84.5 MB |
| RSS real window, idle | 265 MB [263-295] | 248 MB [247-254] |
| release binary unstripped / stripped | 112 / 92 MB (example: gpui test-support + gpui-component) | **54 / 40 MB** |

Delta: the core's `update_components` → re-reduce → relayout is shared; eui-gpui then repaints the whole element tree
(immediate mode), eui-zgui restyles one element and zgui repaints its damage. Now like for like, zgui is 2.7× faster
(the spike's 0.05 ms was signal-only, no core in front).
**Verdict: mixed** — delta, idle CPU, threads, binary better; cold start and RSS equal; fixed-row jump scroll 1.8–2× worse
(element creation cost), RSS grows more while scrolling (+31 MB vs +7 MB; zgui's caches/arenas).

## 5. zgui notes for the write-up

Maintainer status (GitHub API, 2026-11-28): 453 stars, 44 open issues; contributors zortax 809 commits, **Janrupf 214** (second
maintainer, but last commit 2026-10-28), Quick-Flash 19. Last 100 commits (since 2026-10-28): zortax 69, Janrupf 25. Last month:
effectively one active maintainer. HEAD `ee29c7dc` (2026-11-22) unchanged since the spike.

API breaks: none hit (one pinned rev throughout). Friction / missing features / upstream asks:
1. **No text path into a scene built outside zgui-paint** (`zgui-paint/src/emit/text.rs`, `zgui-scene` has no glyph
   builder; `zgui-canvas` is vector-only) → an external painter must go through elements. Upstream ask: a public
   "draw shaped runs" seam (the `GlyphPlacementSource` in `zgui-paint/src/lib.rs` is close).
2. **Element creation cost** dominates windowed lists that are not zgui's own `VirtualList` (~60–70 µs per element incl.
   inline `style_text` parsing through stylo). `For` keyed rows cannot move an element between parents. Ask: a typed,
   pre-parsed style setter (or style objects) and keyed reuse across parents.
3. **`VirtualList` needs a declared fixed row size** (`zgui-ui/src/virtualize`) — no measured / variable rows upstream;
   eui-core's window works around it (criterion 2).
4. **Font matching differs from gpui**: CSS nearest-weight picks a Bold face for weight 600 when only Regular/Bold exist
   (JetBrains Mono); gpui (cosmic-text) draws Regular. zgui is the CSS-correct one; the themes should pin weights that exist.
5. Ergonomics: `.build(cx)` on `AnyView` needs `zgui::view::View` in scope; `ForProps` via `zgui::view`; props types
   generated by `#[component]` must be imported (spike).
6. The reactive runtime / harness is per thread (`Runtime::into_handler`); several surfaces = several subtrees of one
   document (fine), several runtimes on one thread are not supported.
7. Headless: `Harness` + offscreen `WgpuRenderer::read_presented` worked unchanged on Linux (and are the portable path the
   `zshot` binary uses on macOS/Metal); no panic, no crash in any run.
8. Reference-side finding (not zgui): eui-gpui's headless `SurfaceView` paints no in-surface overlay layer (Dialog), so
   headless gpui shots of open overlays are blank.

## Verdicts (Linux)

| criterion | verdict |
|---|---|
| 1. 20 shots match | **not yet** — 10/19 pass (AA-only), Dialog unmeasurable headless on gpui, failures = unported painter details + one font-matching divergence |
| 2. variable-height list, 10k | **go** (within ~20 % of gpui, ≤ 5 ms p95) |
| 3. multiple surfaces | **go** (test passes) |
| 4. harness metrics | **mixed** — delta 2.7×, idle CPU 2×, binary 2.3× better; fixed-row jump scroll ~2× worse |
| 5. zgui status / issues | effectively one active maintainer; no API breaks; 3 upstream asks (text seam, element cost, variable VirtualList) |

Overall (Linux lane): **no blocker in zgui behind eui-core**, but **no-go for the gate as of today**: criterion 1 needs the
remaining painter port (Markdown, Text runs, Input/Select chrome) and criterion 4's scroll regression needs an element-pool
answer (ours or upstream). Note: the interleaved series ran while the machine sat at load 13–190 (other sessions' builds).
