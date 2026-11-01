# `eui-motion`: animations for Exponential UI

The engine behind the eUI `Motion` component (EXP-1584). It loads a Lottie
animation, validates it against the eUI limits (expressions are refused) and
samples any frame into a flat **display list**. Every painter replays that list
and nothing else, so every renderer shows the same picture:

| host | how |
|---|---|
| desktop (gpui), CLI | natively; the `raster` feature (tiny-skia) paints to pixels |
| SwiftUI, Compose | uniffi `MotionPlayer` (packed f32 frames) |
| React, Solid, `<exponential-surface>` | `eui-motion-wasm`, lazy-loaded, one Canvas2D painter |
| `exponential_show` (server) | the same WASM, validation only |

License: Apache-2.0. The engine starts from [velato](https://github.com/linebender/velato)
(Apache-2.0 OR MIT).

## Display list

`src/display.rs` is the contract. Ops: `layer` (an isolated group: blend,
alpha, clip path; composited on `pop`), `clip`, `pop`, `fill`, `stroke`,
`image`. Every op carries its own 2×3 transform; there is no ambient state
besides the layer/clip stack. Colours are straight sRGB RGBA in `0..=1`.
Paths use four verbs (`M`, `L`, `C`, `Z`); quadratics are raised to cubics.
Blends are the 16 W3C modes plus `src-in`, `src-out`, `dst-in`, `dst-out`
(mattes).

### Packed frames

One `f32` stream per frame (WASM `Float32Array`, uniffi `Vec<f32>`):

```
[VERSION=1, width, height, op...]
layer   1, blend, alpha, t[6], path, nFilters, filter*n
clip    2, t[6], path
pop     3
fill    4, t[6], rule(0 nonzero|1 evenodd), brush, path
stroke  5, t[6], width, cap, join, miter, dashOffset, n, dash[n], brush, path
image   6, t[6], asset, alpha, width, height
path    n, verb*n   M 0,x,y | L 1,x,y | C 2,x1,y1,x2,y2,x,y | Z 3
brush   solid 0,r,g,b,a | linear 1,x0,y0,x1,y1,n,(offset,r,g,b,a)*n
        | radial 2,cx,cy,fx,fy,radius,n,(offset,r,g,b,a)*n
filter  blur 0,sigmaX,sigmaY | colorMatrix 1,m[20] | gradientMap 2,amount,n,(offset,r,g,b,a)*n
        | dropShadow 3,r,g,b,a,dx,dy,sigma
blend   normal 0, multiply 1, screen 2, overlay 3, darken 4, lighten 5,
        color-dodge 6, color-burn 7, hard-light 8, soft-light 9, difference 10,
        exclusion 11, hue 12, saturation 13, color 14, luminosity 15,
        dst-in 16, dst-out 17, luma-in 18, luma-out 19, xor 20
cap     butt 0, round 1, square 2      join   miter 0, round 1, bevel 2
```

`luma-in` / `luma-out` multiply what is below by the layer's luminance
(premultiplied Rec. 709) or its complement: track mattes. Filter sigmas and
shadow offsets are in composition units; painters multiply by their scale.

## Engine (M1, EXP-1602)

Our own engine, not a velato dependency (the M0 decision, below):

| module | what |
|---|---|
| `import.rs` | Lottie JSON → model, lenient: unknown layers/shapes skipped, legacy `s`/`e` keyframes, array-wrapped scalars, 0–255 colours, item-level `closed`/`cl`, slots (`sid`) resolved against the file's `slots` and the host's overrides |
| `anim.rs` | keyframes: per-dimension cubic easing, holds, spatial tangents along the arc length |
| `render.rs` | layer time (`st`, `sr`, time remap in seconds, lottie-web's out-point rule), parenting, auto-orient, masks (all modes, inverted, opacity, expansion, feather), track mattes (alpha, luma, inverted, chained, `tp` or the layer above), precomps (clipped), blend modes |
| `shapes.rs` | the After Effects stack: styles paint every path above them, modifiers rewrite paths above them (styles see the final path), repeaters clone everything above them; gradients with alpha stops and highlights; dashes; merge paths (mode 1 = one compound path, add/subtract/intersect/exclude by compositing) |
| `geom.rs`, `modifiers.rs` | lottie-web's ShapePath and primitives; trim (simultaneous / individual), round corners, pucker/bloat, zig-zag, offset path (ported), twist |
| `text/` | text layers as paths: glyphs from `chars` or host fonts (`Motion::add_font`, ttf-parser, kerning), the text document, boxed text, animators with range selectors |
| `effects.rs` | drop shadow, gaussian blur, fill, tint, tritone, stroke → display-list filters |
| `raster.rs` | the tiny-skia reference painter (filters, luma mattes, bounded layer buffers) |

Out of scope by design: expressions (refused) and 3D camera layers (reported).

### Conformance

`packages/exponential-ui/conformance/motion/`: 76 corpus files (lottie-spec,
lottie-web, Skia's skottie resources; sources and licences in
`corpus/SOURCES.md`) rendered by lottie-web (SVG renderer, `render-refs.ts`)
at 3 frames each; `cargo test -p eui-motion --features raster --test corpus`
paints ours with the raster and requires SSIM ≥ 0.98 or an entry in
`known-gaps.json` (a list that may only shrink). Most remaining gaps are on the
reference side (lottie-web ignores mask feather, merge paths, the lighten /
darken / difference mask modes, and shadows on precomp layers) or text drawn with system
fonts. `fixtures/motion-frames.json` pins display lists (JSON + packed) for
the WASM and FFI replays; `fixtures/motion-validate.json` pins refusals;
`catalog/limits.json › motion` the limits.

## Authoring (M2, EXP-1603)

- **Dialect** (`dialect.rs`, `decompile.rs`): readable JSON for people and
  models: ms, SVG path data, CSS easing names, `$color.<token>` (compiled to a
  Lottie slot the host fills from the theme), `{light, dark}` pairs (slot
  `ld:<light>:<dark>`), `{"keys": […]}` on any value, `"lottie": {…}` for raw
  keys. `compile` → Lottie; `decompile` (feature `decompile`, default on, off in
  the WASM) → dialect. The model guide is
  `packages/exponential-ui/eval/motion/guide.md`.
  `tests/dialect.rs`: `decompile(compile(d)) == d` byte for byte and identical
  display lists over the whole corpus.
- **dotLottie** (`dotlottie.rs`): v1 and v2 archives, images inlined, themes →
  slot overrides, state machines kept raw; `write` for our own files.
- **CLI**: `exponential motion import | compile | decompile | check | render
  [--gif]`.
- **WASM**: `MotionPlayer`, `validate`, `compile`; built by
  `scripts/build-motion-wasm.sh` into `packages/exponential-ui/wasm/motion/`
  (committed; `SOURCE_HASH` gates drift). 193 KB gz (`profile.motion-wasm`,
  opt-level z). `@exponential_app/eui/motion`: `motionWasmSync` (servers),
  `loadMotionWasm` (browsers), `motionPropsProblems` (the show validator).
- **LLM eval** (`packages/exponential-ui/eval/motion/`): 30 prompts answered
  with the dialect guide and with an equally short raw-Lottie guide (Sonnet,
  `record.ts`); `tests/eval.rs` scores them: dialect 30/30 valid (28/30 before
  two fixes: `"parent": null` accepted, the eval registers a theme font like
  hosts do), raw Lottie 30/30; mean output 1.1k vs 3.2k tokens.

## Painting a Motion (M3, EXP-1604): the contract for every renderer

A painter paints the packed display list and nothing else. It never
interprets Lottie, the dialect or the clock itself.

1. **Source → Lottie.** `layers` (the dialect: the node's props `size`, `fps`,
   `duration`, `start`, `time`, `layers`, `assets`, `markers` are the document)
   → `compile`; `lottie` as is; `src` (an attachment URL) fetched by the host
   (`.json`, or `.lottie` → `dotlottie::read`, the first animation). A load
   failure shows the poster area empty and records an issue; never a crash.
2. **Load** with slot overrides: every `$color.<token>` slot of the file gets
   the theme's resolved colour (`{"$color.primary": {"p": {"a": 0, "k": [r, g,
   b, a]}}}`); `"$mode": "dark"` in dark mode (resolves `{light, dark}`
   pairs); the node's `slots` prop last. Reload when the theme or mode
   changes. Register the theme's text fonts (`add_font`) for text layers.
3. **Clock**: `set_playback(props JSON)` with `autoplay`, `loop`, `plays`,
   `direction`, `speed`, `segment`, `marker`, `poster`; per frame `tick(ms
   since play started)`. A bound/literal `progress` overrides the clock
   (`progress_frame`). `play: false` freezes the clock where it is.
   Reduced motion, `autoplay: false`, off-screen, hidden window or app in
   the background → `poster_frame()`, no ticking. At most 60 fps (the
   display's rate when lower). At most 8 Motions animate per surface (tree
   order); the rest show their poster. A finished finite loop stops ticking.
4. **Box**: the node's style sizes it; unstyled, the composition's px size,
   shrinking to the available width (aspect kept). `fit`: `contain`
   (default, centred), `cover` (centred, clipped), `fill` (stretched).
   Clip to the box. Transparent background.
5. **Paint the list** in the box with the fit transform, at device
   resolution (`scale × devicePixelRatio`). Ops: see Display list. `layer`
   = an offscreen group (the composition-space clip path under its
   transform), composited on `pop` with `alpha` and `blend`; filters run on
   the group before compositing, sigmas and offsets × the fit scale.
   `dst-in`/`dst-out`/`luma-*` apply wherever the clip's coverage is non-zero
   (no fading at anti-aliased clip edges). Strokes: width, caps, joins,
   miter limit, dashes in the op's local space (the transform scales them).
   Gradients: linear; radial = two-point conical with the focal circle
   radius 0. Images: the decoded asset (data URL or attachment) into
   `0,0,width,height`.
6. **Accessibility**: role image named by `label`; hidden without one;
   never focusable.
7. **Parity**: `conformance/motion/parity/<case>@<frame>.png` are the raster's
   paints of every `fixtures/motion-frames.json` case at 160×160 (contain,
   transparent). Each renderer paints the same packed lists at 160×160 and
   must match within 2/255 per channel on ≥ 99 % of pixels (anti-aliasing
   differs). The clock replays `fixtures/motion-playback.json`.

## Interactivity (M4, EXP-1605): the contract for every renderer

1. **State machine**: the `stateMachine` prop (dotLottie 2 JSON), else a
   `.lottie` `src`'s first state machine → `set_state_machine`. While one
   runs, its current state's playback replaces the playback props.
2. **Inputs**: the `inputs` prop (literal or bound) → `sm_set_inputs` on
   every change (so a data model drives the states).
3. **Interactions** → `sm_post(kind, layer)`: `click` on press (pointer or
   Enter/Space), `pointerDown`/`pointerUp`, `pointerEnter`/`pointerExit` on
   hover, `complete` once when `tick` reports `done` for the current
   state. `layer` = "" (hit-testing layers is not required).
   `sm_post`/`sm_set_inputs` returning true = the state changed: restart
   the clock at 0.
4. **Events** (the node's `on`, A2UI actions): `press` (click; Enter/Space
   when focusable), `hover` (pointer enters), `visible` (first time half
   of it is on screen), `complete` (a finite play finished, or the machine
   reached a final state), `state` (each state `sm_status().entered`
   reports, context `{state}`). A Motion with `on.press` is a button:
   focusable, named by `label`.
5. **Fixture**: `fixtures/motion-state.json` (replayed in Rust, over WASM
   and over uniffi).

## Limits

2 MB source, 5,000 shapes, 120 s, 100,000 vertices per frame. Expressions
(`"x"` on an animated property) are refused with an `expression` issue.

## M0 spike (EXP-1601)

`cargo run --release -p eui-motion --features raster --example spike -- <corpus> <out>`
validates, samples every frame, paints three and prints the table below.
Corpus: the lottie-spec examples and tests (29 files) and lottie-web's test
animations (11 files). Machine: i9-13900K, release build.

### Timings

| | worst | budget |
|---|---|---|
| sample (display list + packing), lottie-spec files | 0.014 ms (one 0.6 ms outlier, first frame of `masks`) | 0.5 ms laptop, 2 ms N95 |
| sample, `bodymovin.json` (1 MB, 156 ops, 993 vertices) | 0.059 ms | 0.5 ms |
| raster paint at 1200×800 (600×400 @2x) | 1.4–5.8 ms; 14 ms for `matte` | 4 ms |

Sampling is two orders of magnitude inside the budget; even a 10× slower
N95 stays under 1 ms. The raster is the gpui and reference path only (the
other five painters draw natively); it allocates a full-size pixmap per layer,
which is what makes mattes slow. M1 bounds layers to their clip rect and
reuses pixmaps.

### WASM size

`eui-motion-wasm` (player + validator), `--profile wasm`:

| build | raw | gzip |
|---|---|---|
| wasm-bindgen output | 738 KB | 233 KB |
| + `wasm-opt -Oz --strip-debug` | 506 KB | 201 KB |

Budget: 200 KB gz. Half of the code (277 KB of 560 KB) is velato's serde
schema (derived deserializers with `flatten` and untagged enums). A
hand-written importer over `serde_json::Value` replaces it in the fork.

### Parity gaps

What the spike engine (velato 0.12 as a crate) gets wrong on the corpus, by
validator code. All are M1 (EXP-1602) work:

- **parsing**: velato's strict serde schema refuses legacy keyframes
  (`s`/`e` pairs, array-wrapped scalar values: `adrock`, `bacon`, `banner`,
  `bm_ronda`, `dalek`) and unknown layer or shape types, which the spec says to
  skip (`unknown-layer`, `unknown-shape`);
- `gap-dashes`: stroke dashes ignored (`gradient-stroke`, `mask`, `masks`, `stroke`, `time_remap`);
- `gap-time-remap`: time remap ignored (`time_remap`);
- `gap-merge-paths`, `gap-pucker-bloat`, `gap-round-corners`: modifiers ignored;
- `gap-luma-matte`: luma mattes drawn as alpha mattes;
- `gap-mask-mode`, `gap-mask-invert`, `gap-mask-expansion`, `gap-mask-feather`:
  only additive masks;
- `gap-text`, `gap-effects`: text layers and layer effects not rendered;
- not in the census: zig-zag, offset path, twist; fill rule (`r`) always
  nonzero; images need host decoding.

Six lottie-web files carry expressions and are refused, as designed.

### Decision: own engine

velato is not a dependency (M1 replaced it):

1. the gaps touch the model, not just the renderer (time remap, text,
   masks, mattes, effects), and velato's public API does not expose the hooks;
2. its strict schema rejects valid real-world files; the spec asks players
   to skip unknown layers and shapes;
3. its schema is half of the WASM; the budget needs a hand-written importer.

Rewriting turned out smaller than forking: the model, importer and
renderer are new; the shape modifiers, text layout and primitive builders
are ports of lottie-web (MIT, `NOTICE`), so the conformance reference and
the engine agree by construction. WASM after M1: 118 KB gz (102 KB after
wasm-opt).

### Not run in the spike

- **N95 / mid Android timings**: no device on this machine; measured on the
  emulator with the FFI in M3.
- **LLM eval (dialect vs raw Lottie, 30 prompts)**: ran in M2 (above).
