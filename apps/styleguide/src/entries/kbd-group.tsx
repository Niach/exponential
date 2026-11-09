import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1629 — THE keycaps: Exponential UI's `Kbd` and `KbdGroup` macros,
// painted by the real React renderer from fixtures/specimens.json
// `exponential-ui-kbd` and `exponential-ui-kbd-group`. The look is the IDE's
// (eui-gpui `controls::kbd`): sans, filled, no border.

export const entry: StyleguideEntry = {
  id: `kbd-group`,
  section: `general`,
  owner: `EXP-1629`,
  title: `Keycaps`,
  blurb: `A shortcut as key caps: Exponential UI's Kbd (one cap) and KbdGroup (one Kbd per key of a chord, 4 px apart). A cap is 20 px tall and at least 20 px wide, 4 px side padding, the radius.sm corner, the accent fill with no border, 12 px medium muted sans text: the IDE's keycap. A bound keys array repeats through a data template.`,
  status: {
    web: { state: `ok`, symbol: `Kbd, KbdGroup (macros)`, file: `packages/exponential-ui/catalog/macros.json`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/kbd_group.rs` },
    ios: { state: `ok`, symbol: `Kbd, KbdGroup (macros)`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `Kbd, KbdGroup (macros)`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => (
    <div className="flex flex-col gap-4">
      <ExponentialUiSpecimen id="exponential-ui-kbd" />
      <ExponentialUiSpecimen id="exponential-ui-kbd-group" />
    </div>
  ),
}
