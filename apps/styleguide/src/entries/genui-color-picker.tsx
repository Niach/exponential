import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #EXP-1760 — Exponential UI's `ColorPicker` macro (a Popover over a grid;
// fixtures/interactions-round1.json `pickerPick`), painted by the real React
// renderer from fixtures/specimens.json `exponential-ui-color-picker`: a pick,
// none, and the open grid. The IDE's `board_form` pickers are the look.

export const entry: StyleguideEntry = {
  id: `genui-color-picker`,
  section: `general`,
  owner: `EXP-1760`,
  title: `Color picker (genUI)`,
  blurb: `A colour field: Exponential UI's ColorPicker, the IconPicker's twin over the label palette. A 32 px outline square shows the colour as a 16 px dot; a press opens the 20 swatches, the pick ringed. A pick writes the hex, fires change and closes.`,
  status: {
    web: { state: `ok`, symbol: `ColorPicker (macro)`, file: `packages/exponential-ui/catalog/macros.json`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/genui_color_picker.rs` },
    ios: { state: `ok`, symbol: `ColorPicker (macro)`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `ColorPicker (macro)`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-color-picker" />,
}
