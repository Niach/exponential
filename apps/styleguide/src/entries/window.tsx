import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// VAPP-290 — THE window frame: Exponential UI's `Window` macro, painted by
// the real React renderer from fixtures/specimens.json
// `exponential-ui-window`. exponential.at frames its live hero and its
// pictures in it.

export const entry: StyleguideEntry = {
  id: `window`,
  section: `general`,
  owner: `VAPP-290`,
  title: `Window`,
  blurb: `A window frame around one child: Exponential UI's Window. Hairline border, radius lg, the card fill and a soft shadow, clipped. A 34 px chrome bar on top (hairline bottom rule, three 10 px muted dots at the start, the optional title centred and muted); the child fills the body. A group named its title; the chrome is hidden from AT.`,
  status: {
    web: { state: `ok`, symbol: `Window (macro)`, file: `packages/exponential-ui/catalog/macros.json`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/window.rs` },
    ios: { state: `ok`, symbol: `Window (macro)`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `Window (macro)`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-window" />,
}
