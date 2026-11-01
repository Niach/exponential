import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1495 — THE anchor list: Exponential UI's `Outline` macro (round-6
// contract §1), painted by the real React renderer from fixtures/
// specimens.json `exponential-ui-outline`. The plan page's left column
// (plan-page.json) is this list; scroll-spy stays in the host.

export const entry: StyleguideEntry = {
  id: `outline`,
  section: `general`,
  owner: `EXP-1495`,
  title: `Outline`,
  blurb: `An anchor list, the plan page's left column: Exponential UI's Outline. An optional UPPERCASE caption, then one 32 px row per item (title, a muted "· n" when count > 0); the active row (item.id = value) gets the accent fill, the foreground title and a 2 px bar at its start edge. A press selects the row and writes value back when bound; no matching value = no active row. Each row is a link, the active one aria-current=location; the list is a navigation landmark.`,
  status: {
    web: { state: `ok`, symbol: `Outline (macro)`, file: `packages/exponential-ui/catalog/macros.json`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/outline.rs` },
    ios: { state: `ok`, symbol: `Outline (macro)`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `Outline (macro)`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-outline" />,
}
