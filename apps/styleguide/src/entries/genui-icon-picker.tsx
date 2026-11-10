import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #EXP-1760 — Exponential UI's `IconPicker` macro (a Popover over a grid;
// fixtures/interactions-round1.json `pickerPick`), painted by the real React
// renderer from fixtures/specimens.json `exponential-ui-icon-picker`: a pick,
// none, and the open grid. The IDE's `board_form` pickers are the look.

export const entry: StyleguideEntry = {
  id: `genui-icon-picker`,
  section: `general`,
  owner: `EXP-1760`,
  title: `Icon picker (genUI)`,
  blurb: `An icon field: Exponential UI's IconPicker, the identity picker of boards, agents and labels. A 32 px outline square shows the pick in its colour (a dashed placeholder while none); a press opens a popover grid of the board icons (or the offered set), eight to a row, scrolling past 320 px, the pick ringed. A pick writes the value, fires change and closes; allowNone adds a No icon row.`,
  status: {
    web: { state: `ok`, symbol: `IconPicker (macro)`, file: `packages/exponential-ui/catalog/macros.json`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/genui_icon_picker.rs` },
    ios: { state: `ok`, symbol: `IconPicker (macro)`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `IconPicker (macro)`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-icon-picker" />,
}
