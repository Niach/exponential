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
layer   1, blend, alpha, t[6], path
clip    2, t[6], path
pop     3
fill    4, t[6], rule(0 nonzero|1 evenodd), brush, path
stroke  5, t[6], width, cap, join, miter, dashOffset, n, dash[n], brush, path
image   6, t[6], asset, alpha, width, height
path    n, verb*n   M 0,x,y | L 1,x,y | C 2,x1,y1,x2,y2,x,y | Z 3
brush   solid 0,r,g,b,a | linear 1,x0,y0,x1,y1,n,(offset,r,g,b,a)*n
        | radial 2,cx,cy,fx,fy,radius,n,(offset,r,g,b,a)*n
blend   normal 0, multiply 1, screen 2, overlay 3, darken 4, lighten 5,
        color-dodge 6, color-burn 7, hard-light 8, soft-light 9, difference 10,
        exclusion 11, hue 12, saturation 13, color 14, luminosity 15,
        src-in 16, src-out 17, dst-in 18, dst-out 19
cap     butt 0, round 1, square 2      join   miter 0, round 1, bevel 2
```

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

### Decision: fork

velato is forked into this crate (M1), not used as a dependency:

1. the gaps touch the model, not just the renderer (time remap, text,
   masks, mattes, effects), and velato's public API does not expose the hooks;
2. its strict schema rejects valid real-world files; the spec asks players
   to skip unknown layers and shapes;
3. its schema is half of the WASM; the budget needs a hand-written importer.

The fork keeps velato's runtime model and renderer under their original
Apache-2.0 OR MIT headers, drops `vello`, `peniko`'s brush types and the serde
schema, and records upstream in `NOTICE`.

### Not run in the spike

- **N95 / mid Android timings**: no device on this machine; measured on the
  emulator with the FFI in M3.
- **LLM eval (dialect vs raw Lottie, 30 prompts)**: the dialect is approved;
  the eval needs its compiler and runs in M2 (EXP-1603), where it gates the
  catalog prompt (≥ 95% valid on the first try).
