import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #EDE-65 — genUI's `Embed`: a nested surface drawn in place, sandboxed
// (fixtures/embed.json), painted by the real React renderer from
// fixtures/specimens.json `exponential-ui-embed`: the example, a result card
// with its own data, a question card whose reply reaches the host, a bad
// surface's error box.

export const entry: StyleguideEntry = {
  id: `embed`,
  section: `general`,
  owner: `EDE-65`,
  title: `Embed`,
  blurb: `A genUI card inside a genUI surface: Exponential UI's Embed. \`surface\` is {root, data?}: a nested component tree with its own data model, drawn in place. It is sandboxed: its bindings read its own data, never the host's; its \`set\` writes only its own model; its ids never meet the host's; an inner host function is refused. Every inner \`event\` action reaches the host as the Embed's \`action\` {name, context}. A surface that is not a card shows an error box (This card could not be shown) and the host around it keeps painting. No role of its own: the nested controls join the host's tab order in place.`,
  status: {
    web: { state: `ok`, symbol: `Embed (native)`, file: `packages/exponential-ui-react/src/natives`, note: `here: ExponentialUiSpecimen (packages/ui); the shared half: packages/exponential-ui/src/embed.ts` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/embed.rs` },
    ios: { state: `ok`, symbol: `Embed (native)`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `Embed (native)`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-embed" />,
}
