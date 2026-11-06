import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #VAPP-285 — THE tree: genUI's `Tree` macro (round-7 contract), painted by
// the real React renderer from fixtures/specimens.json `exponential-ui-tree`.
// One Row per visible node with the tree guides; replaces Row depth +
// TreeGuides for Files and Version history.

export const entry: StyleguideEntry = {
  id: `tree`,
  section: `general`,
  owner: `VAPP-285`,
  title: `Tree`,
  blurb: `A file-style tree: genUI's Tree. One compact Row per visible node, indented by depth with the tree guides; a branch leads with a chevron (right closed, down open) and a folder, a leaf with a file. A branch press folds it (toggle), a leaf press selects it (select, value written back when bound); a branch with hasChildren and no children loads on open (load) and shows a loading row. Keyboard per the WAI-ARIA tree: Up/Down, Home/End, Right opens or enters, Left closes or goes to the parent.`,
  status: {
    web: { state: `ok`, symbol: `Tree (macro)`, file: `packages/exponential-ui/catalog/macros.json`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/tree.rs` },
    ios: { state: `ok`, symbol: `Tree (macro)`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `Tree (macro)`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-tree" />,
}
