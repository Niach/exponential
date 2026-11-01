import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1495 — THE wide picture group: Exponential UI's `Mosaic` macro
// (round-6 contract §2), painted by the real React renderer from fixtures/
// specimens.json `exponential-ui-mosaic`. The plan page's wide picture
// group (plan-page.json mosaicTiles) is this; phones page a Carousel.

export const entry: StyleguideEntry = {
  id: `mosaic`,
  section: `general`,
  owner: `EXP-1495`,
  title: `Mosaic`,
  blurb: `A wide picture group at 1.6:1: Exponential UI's Mosaic. One picture fills the group, two split it in halves, three or more put the first at 60 % beside a column of two, and past three the last tile darkens under a "+N". Every tile is a button named its picture's alt (radius lg, hairline border, the muted fill under the picture, hover dims to 0.92); a press fires select {index} and the host opens its lightbox over every picture.`,
  status: {
    web: { state: `ok`, symbol: `Mosaic (macro)`, file: `packages/exponential-ui/catalog/macros.json`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/mosaic.rs` },
    ios: { state: `ok`, symbol: `Mosaic (macro)`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `Mosaic (macro)`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-mosaic" />,
}
