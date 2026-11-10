import { ExponentialUiTree } from "@exp/ui"
import type { NestedNode } from "@exponential_app/eui"

import specimens from "@exponential_app/eui/fixtures/motion.specimens.json"
import type { StyleguideEntry } from "./types.ts"

// EXP-1604 — Exponential UI's `Motion`: a vector animation painted from the
// engine's display list (eui-motion README.md › Painting a Motion). The four
// specimens live in motion.specimens.json, which the desktop twin reads too:
// `cases` in the page's mode, `illustration` once per mode. The island is
// static markup; the page's Motion script (motion-islands.ts) mounts the
// shared web painter on every Motion canvas, WASM inlined. EXP-1605:
// `interactive` = the state-machine specimens (a like button toggled by
// clicks, a level bar whose states follow its bound `inputs`); the script
// hands the painter the press / hover / pointer interactions.

const SPECIMENS = specimens as unknown as { data: Record<string, unknown>; cases: NestedNode; illustration: NestedNode; interactive?: NestedNode }

function Illustration({ mode, caption }: { mode: `light` | `dark`; caption: string }) {
  return (
    <div className="flex flex-col gap-2">
      <span className="text-xs text-muted-foreground">{caption}</span>
      <ExponentialUiTree id={`motion-illustration-${mode}`} node={SPECIMENS.illustration} data={SPECIMENS.data} mode={mode} />
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `motion`,
  section: `general`,
  owner: `EXP-1604`,
  title: `Motion`,
  blurb: `A vector animation drawn the same everywhere: loaders, success states, illustrated empty states. Exponential UI's Motion takes the dialect (layers in ms, \`$color\` tokens, \`{light, dark}\` pairs), a Lottie file or an attached .json/.lottie; the engine (eui-motion) samples each frame into one display list and every platform paints that list and nothing else. \`$color\` slots take the theme's colours and follow the mode. The clock is the engine's: at most 60 fps, paused off-screen and in a hidden window, at most 8 playing per surface, the poster under reduced motion; \`progress\` scrubs it (bind it to drive the animation from data), \`play: false\` freezes it. Unstyled, the box is the composition's px size, shrinking to the width; \`fit\` contain, cover or fill. A label names it for screen readers; without one it is decorative. A \`stateMachine\` (dotLottie, run by the engine) makes it a control: clicks, hovers and the bound \`inputs\` move its states, and \`on\` reports press, hover, visible, complete and every entered state; with \`on.press\` it is a button.`,
  status: {
    web: { state: `ok`, symbol: `MotionNative`, file: `packages/exponential-ui-react/src/natives/motion.tsx`, note: `painter: MotionView + paintMotionFrame (packages/exponential-ui/src/web/primitives)` },
    desktop: { state: `ok`, symbol: `MotionPlayer`, file: `apps/desktop/crates/eui-gpui/src/motion.rs`, note: `the raster (tiny-skia) into a RenderImage; twin: apps/desktop/crates/ui/src/styleguide/entries/motion.rs` },
    ios: { state: `leftover`, symbol: `MotionPlayer`, file: `apps/desktop/crates/exponential-ui-ffi/src/motion.rs`, note: `the SwiftUI painter lands in its own EXP-1604 PR` },
    android: { state: `leftover`, symbol: `MotionPlayer`, file: `apps/desktop/crates/exponential-ui-ffi/src/motion.rs`, note: `the Compose painter lands in its own EXP-1604 PR` },
  },
  island: () => (
    <div className="flex flex-col gap-6">
      <ExponentialUiTree id="motion" node={SPECIMENS.cases} data={SPECIMENS.data} />
      {SPECIMENS.interactive ? <ExponentialUiTree id="motion-interactive" node={SPECIMENS.interactive} data={SPECIMENS.data} /> : null}
      <div className="grid grid-cols-2 gap-4">
        <Illustration mode="light" caption="Light" />
        <Illustration mode="dark" caption="Dark" />
      </div>
    </div>
  ),
}
