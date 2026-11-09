---
name: exponential-motion
description: "Animate with Motion: the readable Motion dialect that compiles 1:1 to Lottie, Lottie and dotLottie (.lottie) sources, playback, dotLottie state machines with inputs and events, the exponential motion import/compile/decompile/check/render CLI, and eight worked examples. Use when adding a Motion component to a surface or an app, or converting a Lottie file."
---

# Motion

> Generated from docs/spec/motion.md and the Motion guide (genUI 0.21.0) — do not edit; regenerate with packages/exponential-ui/scripts/generate-skills.ts.

`Motion` is the genUI component for vector animation. **Lottie JSON is the
stored and rendered format.** Models and people write a readable **dialect**
that compiles 1:1 to Lottie and converts back without loss. One Rust engine,
`eui-motion`, samples any frame into a flat **display list**; all six
renderers (React, Solid, the element, gpui, SwiftUI, Compose) paint that list
and nothing else, so every platform shows the same pixels. The same engine
runs the playback clock and dotLottie state machines, natively, over WASM and
over uniffi. Expressions are refused; 3D camera layers are reported, not
rendered.

## Sources

A Motion node takes exactly one source:

| prop | what | checked by |
|---|---|---|
| `layers` (+ `size`, `duration`, `fps`, `start`, `time`, `assets`, `markers`) | the dialect, inline: the node's props ARE the document | `compile` |
| `lottie` | Lottie JSON, inline | `validate` |
| `src` | an attachment (`/api/attachments/{id}`), `.json` or `.lottie` | the host's media policy, then `validate` |

Public URLs (LottieFiles…) are outside the host URL policy. `exponential_show`
validates every Motion of a surface with the engine (the WASM, server-side)
and refuses the card with each problem's code and JSON pointer.

## The dialect (what you write)

Motion animation JSON (compiles to Lottie). Root: {"size":[w,h], "duration":ms, "fps":60, "layers":[…]}. Layers are listed TOP first; each draws in the composition's pixel space (origin top-left).

Layer: {"type":"shape", "id":"name", "transform":{…}, "shapes":[…]}. Other types: "solid" {color, size}, "null" (a parent for others), "precomp" {asset, size} with "assets":{"id":{"layers":[…]}}, "text" {text:{text, font, size, color, align}}. Optional: "parent":"id", "in"/"out" (ms), "blend":"multiply"|"screen"…, "matte":{"source":"id","mode":"alpha"|"alpha-inverted"|"luma"|"luma-inverted"}, "masks":[{"path":"M…Z","mode":"add"|"subtract"|"intersect","inverted":true}].

transform (layers and groups): anchor [x,y], position [x,y], scale [100,100] (percent), rotation (degrees, clockwise), opacity (0–100), skew, skewAxis.

shapes, drawn top first, a style paints every shape listed BEFORE it in the same group:
- geometry: {"type":"rect","position":[x,y],"size":[w,h],"radius":r} (position = centre), {"type":"ellipse","position","size"}, {"type":"star","points":5,"outerRadius","innerRadius"}, {"type":"polygon","points","outerRadius"}, {"type":"path","d":"M0 0 L10 0 C… Z"} (SVG path data);
- styles: {"type":"fill","color"}, {"type":"stroke","color","width","cap":"round","join":"round","dash":[len,gap]}, {"type":"gradient-fill","kind":"linear"|"radial","start":[x,y],"end":[x,y],"stops":[[0,"#fff"],[1,"#000"]]};
- modifiers: {"type":"trim","start":0,"end":100,"offset":0} (percent; draw-on lines), {"type":"repeater","copies":n,"transform":{…}}, {"type":"round-corners","radius"};
- {"type":"group","shapes":[…],"transform":{…}} nests and transforms its own shapes.
Put shapes inside a group when they need their own transform.

Colours: "#rrggbb", "#rrggbbaa", a theme token "$color.primary" (also foreground, muted, accent, destructive, success, warning, info, chart1…chart8) that follows light/dark mode, or {"light":"#000","dark":"#fff"}. Prefer tokens.

Animate ANY value with keys: {"keys":[{"t":0,"v":[0,0],"ease":"ease-in-out"},{"t":600,"v":[100,0]}]}. t in ms, v the value at t, ease (for the segment that starts at this key): linear (default), ease, ease-in, ease-out, ease-in-out, hold, or [x1,y1,x2,y2]. Paths animate too (same command count in every key). Tokens cannot be animated.

Keep it small: a few layers, round numbers, ≤ 2 s loops, start and end on the same pose for loops. Never use expressions.

Put the document in a `Motion` component's props (the node's props ARE the document), with `label` for accessibility. The Motion component's own props (playback, inputs, stateMachine, on): exponential_apps {op: "catalog", only: ["Motion"]}.

## Tooling

`exponential motion import | compile | decompile | check | render [--frame N |
--at ms] [--size WxH] [--gif]`. `@exponential_app/eui/motion` (browsers, lazy)
and `…/motion/server` (`motionPropsProblems`, sync). The WASM build
(`scripts/build-motion-wasm.sh`, ≤ 200 KB gz, committed with a source-hash
gate).

## References (read on demand)

- `references/spec.md` — the whole spec: limits, the dialect's full grammar, playback, interactivity (state machines, inputs, events), out of scope.
- `references/lottie.md` — writing or editing raw Lottie JSON instead of the dialect.

## Worked examples (dialect documents)

- `examples/pulse-dot.json` — A live indicator: a small dot with a ring that pulses outward and fades, looping, 32×32.
- `examples/progress-ring.json` — A progress ring that fills from 0 to 75% over one second, 80×80.
- `examples/typing-dots.json` — Three typing-indicator dots that rise and fall one after another, 60×20.
- `examples/heartbeat.json` — A heart that beats twice and rests, looping, 64×64.
- `examples/equalizer.json` — A music equalizer with five bars moving up and down at different speeds, 80×60.
- `examples/error-x.json` — An error state: a red circle pops in and an X draws itself inside, 64×64.
- `examples/sun.json` — A sun with rays that grow and shrink while the sun rotates, 120×120.
- `examples/wifi.json` — A wifi icon whose three arcs light up one after another, 64×64.
