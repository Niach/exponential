import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// VAPP-280 (round 7) — THE settings form row: Exponential UI's `SettingsRow`
// macro (docs/round-7-contract.md), painted by the real React renderer from
// fixtures/specimens.json `exponential-ui-settings-row`: a Notifications
// group (Group over a bare Section) with a Switch, a Select and a Slider row,
// then the stacked phone layout.

export const entry: StyleguideEntry = {
  id: `settings-row`,
  section: `general`,
  owner: `VAPP-280`,
  title: `Settings row`,
  blurb: `One line of a settings page: Exponential UI's SettingsRow. The label (body) with an optional muted 12 px description on the start column, the control (its children) on the end column, 16 px in from the sides, 12 px top and bottom, at least 44 px tall: the app's settings row. Below stackBelow (default sm) the control goes under the label and stretches; never keeps a Switch beside its label on a phone. A page is a Group holding a bare Section, so rows get the card and the hairlines. The row is a group named by its label and described by its description; the control needs no label of its own.`,
  status: {
    web: { state: `ok`, symbol: `SettingsRow (macro)`, file: `packages/exponential-ui/catalog/macros.json`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/settings_row.rs` },
    ios: { state: `ok`, symbol: `SettingsRow (macro)`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `SettingsRow (macro)`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-settings-row" />,
}
