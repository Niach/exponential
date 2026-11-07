import { ExponentialUiTree } from "@exp/ui"
import type { NestedNode } from "@exponential_app/eui"

import specimens from "./canvas.specimens.json"
import type { StyleguideEntry } from "./types.ts"

// Round 8 — genUI's `Canvas`: a static vector drawing (charts, plots,
// diagrams) compiled by Motion's engine into one display list, its text set
// natively in the theme's fonts. The specimens live in canvas.specimens.json,
// which the desktop twin reads too: a bar chart, a line plot and a pointer
// pad. The island is static markup; the page's script (motion-islands.ts)
// mounts the shared web painter (`CanvasView`) on every Canvas and writes the
// pad's last press into its readout.

const SPECIMENS = specimens as unknown as { data: Record<string, unknown>; cases: NestedNode }

export const entry: StyleguideEntry = {
  id: `canvas`,
  section: `general`,
  owner: `FEED-197`,
  title: `Canvas`,
  blurb: `A static vector drawing painted the same everywhere: charts, plots, diagrams, small illustrations. \`size\` is the drawing's coordinate space; \`shapes\` takes Motion's shape vocabulary (rect, ellipse, star, polygon, path; fill, stroke, gradient-fill; trim, repeater, round-corners; group) plus \`text\`, set on one line in the theme's sans or mono font. Colours are \`$color\` tokens or \`{light, dark}\` pairs, so the drawing follows the theme and the mode. Unstyled, the box is the drawing's px size, shrinking to the width; \`fit\` contain, cover or fill. A label names it for screen readers; without one it is decorative. \`pointer: true\` reports press, move and leave in drawing units.`,
  status: {
    web: { state: `ok`, symbol: `CanvasNative`, file: `packages/exponential-ui-react/src/natives/canvas.tsx`, note: `painter: CanvasView + paintMotionFrame (packages/exponential-ui/src/web/primitives)` },
    desktop: { state: `ok`, symbol: `CanvasPlayer`, file: `apps/desktop/crates/eui-gpui/src/canvas.rs`, note: `the raster (tiny-skia) + native text on top` },
    ios: { state: `ok`, symbol: `CanvasLeaf`, file: `packages/exponential-ui-swift/Sources/ExponentialUI/Views/Leaves/CanvasLeaf.swift`, note: `the uniffi CanvasPlayer, painted by MotionPainter` },
    android: { state: `ok`, symbol: `CanvasLeaf`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose/CanvasLeaf.kt`, note: `the uniffi CanvasPlayer, painted by MotionPainter` },
  },
  island: () => <ExponentialUiTree id="canvas" node={SPECIMENS.cases} data={SPECIMENS.data} />,
}
