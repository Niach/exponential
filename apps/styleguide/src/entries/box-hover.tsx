import { ExponentialUiTree } from "@exp/ui"
import type { NestedNode } from "@exponential_app/eui"

import specimens from "./box-hover.specimens.json"
import type { StyleguideEntry } from "./types.ts"

// #EDE-65 — Exponential UI's Box `on.hover`: fired as the pointer enters the
// box, exactly like Motion's hover (no payload; leaving fires nothing). The
// specimen lives in box-hover.specimens.json, which the desktop twin reads
// too: three tiles whose hover sets /selected (the eDE Alt-Tab switcher's
// hover-select), the selected one on `$color.accent`.

const SPECIMENS = specimens as unknown as { data: Record<string, unknown>; tiles: NestedNode }

export const entry: StyleguideEntry = {
  id: `box-hover`,
  section: `general`,
  owner: `EDE-65`,
  title: `Box hover`,
  blurb: `A Box reports the pointer: \`on.hover\` fires when the pointer enters it, like Motion's hover. No payload (the action carries its own context, a template item's relative paths read the hovered item); leaving fires nothing, entering again fires again. The Box need not be pressable; a pressable one still presses. The eDE Alt-Tab switcher selects the hovered tile this way, as Mint's does.`,
  status: {
    web: { state: `ok`, symbol: `BoxNative`, file: `packages/exponential-ui-react/src/natives/layout.tsx`, note: `Solid: packages/exponential-ui-solid/src/natives/layout.tsx` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/box_hover.rs`, note: `the hover: apps/desktop/crates/eui-gpui/src/view/paint.rs` },
    ios: { state: `ok`, symbol: `Box`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `Box`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => <ExponentialUiTree id="box-hover" node={SPECIMENS.tiles} data={SPECIMENS.data} />,
}
