---
title: Motion
description: "genUI's vector animations: one Rust engine samples every frame into a display list, and every renderer paints that list, so React, Solid, the web component, SwiftUI, Compose and gpui show the same picture."
---

`Motion` is the genUI component for vector animation: a loader, a success state, an illustrated empty state, a small explainer. It plays the same on every platform genUI renders on. See it live on [/motion/](/sdk/motion/).

## How it works

| Part | What it does |
|---|---|
| The dialect | Readable JSON (ms, SVG path data, CSS easing names, `$color` tokens) that a person or a model writes and that compiles to Lottie, losslessly both ways. |
| The engine | `eui-motion` (Rust) loads a Lottie animation, validates it against the genUI limits (expressions are refused) and samples each frame into a flat display list. |
| The players | Each renderer paints the display list: one Canvas2D painter for React, Solid and the web component (the engine runs as WebAssembly, about 200 KB gzipped, loaded on the first Motion a page paints), SwiftUI and Compose through UniFFI, gpui natively. |
| The state machine | dotLottie 2 state machines turn an animation into a control; the engine interprets them, the renderer reports pointer input and restarts the clock. |

## The web player

A custom web renderer paints Motion, Canvas and the VoiceWave with the shared player from `@exponential_app/eui/web/motion` (`MotionView`, `CanvasView`, `paintMotionFrame`, `paintText`, `voiceWaveClock` and the rest). React, Solid and the web component use it already.

:::caution[Moved]
These exports used to come from `@exponential_app/eui/web`, which no longer has them. Change `import { MotionView } from "@exponential_app/eui/web"` to `import { MotionView } from "@exponential_app/eui/web/motion"`. Nothing else changes.
:::

## Sources

A `Motion` node takes exactly one source:

| Prop | What |
|---|---|
| `layers` (+ `size`, `duration`, `fps`, `assets`, `markers`) | The [dialect](/docs/sdk/motion/reference/dialect/), inline in the surface. |
| `lottie` | A Lottie JSON animation as is. Expressions are refused; 3D layers are skipped. |
| `src` | An attached `.json` or `.lottie` file (the first animation of a `.lottie`, its images inlined), fetched under the host's media policy. |

## Models write it

The dialect's guide is part of `catalogPrompt()`, so an agent that builds genUI surfaces can animate them with no extra setup. In the eval, a model wrote all 30 prompts as valid animations on its first try, in about a third of the tokens raw Lottie takes.

## Where next

- [Animate with Motion](/docs/sdk/motion/guides/animate/): the guide, with live examples.
- [Specification](/docs/sdk/motion/reference/), [Dialect](/docs/sdk/motion/reference/dialect/), [Engine and players](/docs/sdk/motion/reference/engine/).
- The component's props: [Motion in the genUI reference](/docs/sdk/genui/reference/components/motion/).
