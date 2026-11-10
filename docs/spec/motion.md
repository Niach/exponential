# Motion: spec

> Status: **shipped** (EXP-1584: M0 #1622, M1 #1623, M2 #1624, M3 #1638, M4 #1648; genUI 0.4.0). Design approved 2026-11-01 (Danny; the comment on #EXP-1584).
> Code: `apps/desktop/crates/eui-motion` (the engine), `eui-motion-wasm`, `exponential-ui-ffi` (`MotionPlayer`), the `Motion` painters in every genUI renderer.
> Fixtures (`packages/exponential-ui/fixtures/`): `motion-frames.json`, `motion-validate.json`, `motion-playback.json`, `motion-state.json`; corpus `conformance/motion/`.

## 0. In one paragraph

`Motion` is the genUI component for vector animation. **Lottie JSON is the
stored and rendered format.** Models and people write a readable **dialect**
that compiles 1:1 to Lottie and converts back without loss. One Rust engine,
`eui-motion`, samples any frame into a flat **display list**; all six
renderers (React, Solid, the element, gpui, SwiftUI, Compose) paint that list
and nothing else, so every platform shows the same pixels. The same engine
runs the playback clock and dotLottie state machines, natively, over WASM and
over uniffi. Expressions are refused; 3D camera layers are reported, not
rendered.

```
dialect ──compile──▶ Lottie JSON ◀── .json / .lottie (dotLottie)
                         │
                   eui-motion (import → model → sample(frame))
                         │
                  display list (packed f32)
      ┌──────────┬───────┴──────┬──────────┬─────────┐
   Canvas2D    gpui (raster)  SwiftUI   Compose   tiny-skia
 (React/Solid/                Canvas    DrawScope  reference
   element)
```

## 1. Sources

A Motion node takes exactly one source:

| prop | what | checked by |
|---|---|---|
| `layers` (+ `size`, `duration`, `fps`, `start`, `time`, `assets`, `markers`) | the dialect, inline: the node's props ARE the document | `compile` |
| `lottie` | Lottie JSON, inline | `validate` |
| `src` | an attachment (`/api/attachments/{id}`), `.json` or `.lottie` | the host's media policy, then `validate` |

Public URLs (LottieFiles…) are outside the host URL policy. `exponential_show`
validates every Motion of a surface with the engine (the WASM, server-side)
and refuses the card with each problem's code and JSON pointer.

## 2. Limits (`catalog/limits.json › motion`)

2 MB source · 5,000 shapes · 100,000 vertices per frame · 120 s · ≤ 60 fps ·
≤ 8 animating per surface. `validate` refuses `expression` (a string `x` on an
animated property), `limit-bytes`, `limit-shapes`, `limit-duration`,
`timing` (no `fr`/`op`), `parse`; it warns (`camera`, `3d-layer`, `effect`
for unsupported effect types, `text-path`).

## 3. The dialect

Readable JSON a model writes with ~600 tokens of guidance (the catalog's
`Motion.layers` description; `packages/exponential-ui/eval/motion/guide.md`).

**Root**: `size` [w, h] px, `duration` ms, `fps` (60), `start` ms, `time`
(`"ms"` default, `"frames"`), `name`, `layers`, `assets` {id: {layers} |
{image, size}}, `markers` [{name, time, duration}], `lottie` {raw root keys}.

**Layer**: `type` shape | solid | image | null | precomp | text | lottie (raw),
`id` (a string or number; parents and mattes refer to it), `name`, `parent`,
`in` / `out` / `start` (time units), `stretch`, `hidden`, `autoOrient`,
`blend` (the 16 W3C modes), `transform`, `matte` {source, mode: alpha |
alpha-inverted | luma | luma-inverted}, `masks` [{path, mode: add | subtract |
intersect | lighten | darken | difference | none, inverted, opacity, expansion,
feather}], then per type: `shapes`; `color` + `size` (solid); `asset` (+ `size`,
`timeRemap` for precomps); `text` {text, font, size, color, strokeColor,
strokeWidth, align, tracking, lineHeight, box, animators}; `lottie` {raw keys}.

**Transform** (layers, groups, repeaters): `anchor`, `position` (or
`positionX`/`positionY`), `scale` %, `rotation` deg clockwise, `opacity`
0–100, `skew`, `skewAxis`, `startOpacity`/`endOpacity` (repeaters), `lottie`.

**Shapes** (drawn top first; a style paints every shape before it in its
group, modifiers rewrite them): `group` {shapes, transform}, `rect` {position,
size, radius, direction}, `ellipse`, `star` / `polygon` {points, outerRadius,
innerRadius, outerRoundness, innerRoundness, rotation}, `path` {d: SVG path
data, any commands incl. arcs; several subpaths allowed}, `fill` {color,
opacity, rule}, `stroke` {color, width, opacity, cap, join, miterLimit, dash,
dashOffset}, `gradient-fill` / `gradient-stroke` {kind linear | radial, start,
end, stops [[offset, colour]…], highlight, highlightAngle}, `trim` {start, end,
offset, mode}, `repeater` {copies, offset, composite, transform},
`round-corners`, `pucker-bloat`, `zig-zag`, `offset-path`, `twist`, `merge`
{mode merge | add | subtract | intersect | exclude}, `lottie` (raw).

**Values**: a number, an array, a colour, path data, or animated
`{"keys": [{t, v, ease, end?, to?, ti?}, …]}`: `t` in the time unit, `ease`
(of the segment starting at the key) linear (default) | ease | ease-in |
ease-out | ease-in-out | hold | [x1, y1, x2, y2] | {o, i} (per dimension);
`to`/`ti` spatial tangents.

**Colours**: `#rgb`, `#rrggbb`, `#rrggbbaa`, `[r, g, b(, a)]` 0–1,
`$color.<token>` (a theme token: compiled to a Lottie slot `sid` the host
fills with the theme's resolved colour, follows light/dark), or
`{light, dark}` (slot `ld:<light>:<dark>`, resolved by the host's `$mode`).
Tokens and pairs cannot be animated.

**Round trip** (`tests/dialect.rs`, over the whole corpus): `decompile(compile(d))
== d` byte for byte for every canonical document; `compile(decompile(l))`
samples the same display lists as `l`. What the friendly fields do not name
(effects, 3D, foreign slots) rides in `lottie` pass-throughs. Old exports'
`closed`/`cl`, legacy `s`/`e` keyframes and a closing vertex that duplicates
the first are preserved.

**Model eval** (`eval/motion/`, 30 prompts, Sonnet): the dialect 30/30 valid
on the first try, raw Lottie 30/30 at ~3× the output tokens.

## 4. The engine

- **Import** (lenient): unknown layers and shapes skipped (the spec asks it),
  legacy keyframes, array-wrapped scalars, 0–255 colours, slots resolved
  against the file's `slots`, the host's overrides and `$mode`.
- **Sample**: lottie-web's time model (`st`, `sr`, time remap in seconds, the
  out-point rule), parenting, auto-orient, masks (every mode, inverted,
  opacity, expansion, feather), alpha / luma / inverted / chained mattes,
  precomps (clipped), blend modes, the After Effects shape stack (styles see
  the final paths), modifiers ported from lottie-web, gradients with alpha
  stops and highlights, dashes, text layers as paths (the file's `chars` or
  host fonts with kerning; animators with range selectors), effects (drop
  shadow, blur, fill, tint, tritone, stroke) lowered to four filters.
- **Conformance** (`conformance/motion/`): 76 licensed files rendered by
  lottie-web at 3 frames; SSIM ≥ 0.98 or a reasoned entry in
  `known-gaps.json` (mostly features lottie-web lacks).

## 5. The display list

Ops `layer` (isolated group: blend, alpha, clip path, filters; composited on
`pop`), `clip`, `pop`, `fill` (nonzero | evenodd), `stroke` (width, caps,
joins, miter, dashes), `image`. Every op carries its own 2×3 transform; paths
use M / L / C / Z; colours are straight sRGB. Blends: the 16 W3C modes,
`dst-in`, `dst-out`, `luma-in`, `luma-out`, `xor`. Filters: blur, colour
matrix, gradient map, drop shadow (sigmas in composition units).
Crosses boundaries as a packed `f32` stream (`[1, w, h, op…]`, the layout in
the crate README) or JSON (fixtures). `motion-frames.json` pins 24 cases; the
raster's paints of them (`conformance/motion/parity/`) are every painter's
pixel reference.

## 6. Playback

One clock (`playback.rs`), pinned by `motion-playback.json`:
`autoplay`, `loop` / `plays`, `direction` forward | reverse | alternate,
`speed`, `segment` [from, to] ms or `marker`, `poster` ms; `progress` (0–1,
bindable) scrubs instead; `play: false` freezes. Hosts paint ≤ 60 fps,
pause off-screen and in the background, keep ≤ 8 animating per surface (tree
order), show the poster under reduced motion. Unstyled, the box is the
composition's px size, shrinking to the width; `fit` contain | cover | fill.

## 7. Interactivity

dotLottie 2 state machines (`state_machine.rs`, pinned by `motion-state.json`):
playback states (segment or marker, loop, speed, mode, autoplay, final),
transitions with guards (numeric, string, boolean, event) on `inputs`,
interactions (click, pointer down/up/enter/exit, complete, loop complete,
optionally per layer) running actions (fire, set, increment, decrement,
toggle, reset), entry/exit actions. genUI maps them:

| genUI | state machine |
|---|---|
| `stateMachine` prop, or a `.lottie` src's first machine | the machine |
| `inputs` (literal or bound to the data model) | input writes |
| press / Enter / Space | `click` |
| hover | `pointerEnter` / `pointerExit` |
| the state's playback finished | `complete` (its guard-less transition) |

Events (`on`): `press`, `hover`, `visible` (first time half on screen),
`complete`, `state` (context `{state}`, once per entered state). A Motion with
`on.press` is a button for accessibility; otherwise an image named by `label`,
decorative without one.

## 8. Tooling

`exponential motion import | compile | decompile | check | render [--frame N |
--at ms] [--size WxH] [--gif]`. `@exponential_app/eui/motion` (browsers, lazy)
and `…/motion/server` (`motionPropsProblems`, sync). The WASM build
(`scripts/build-motion-wasm.sh`, ≤ 480 KB raw (about 200 KB gz), committed with a source-hash
gate).

## 9. Out of scope

Expressions (refused), 3D camera layers (reported), text on a path (drawn on
its baseline), the Gradient Ramp and other generator effects, public URLs as
sources.
