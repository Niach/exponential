import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// VAPP-217 (eApps U2) — Exponential UI's `RecordTable` macro, painted by the
// real React renderer from fixtures/specimens.json `exponential-ui-record-table`
// (the catalog example and the Autotax Expenses ledger, host-filled props
// included: src/host/record-macros.ts).

export const entry: StyleguideEntry = {
  id: `record-table`,
  section: `general`,
  owner: `VAPP-217`,
  title: `Record table`,
  blurb: `The records of one kind: Exponential UI's RecordTable macro over the stock Table (header band, row hairlines, compact, sticky header, sortable). A toolbar (an optional search field and enum filter Select, the count, Add) over the Data tab's typed columns (date 104, time 64, date-time 152, number 64, currency 96, boolean 56 px, text shares the rest; a column object sets the label, type, currency, decimals, date format, align, width), the relative Updated column, an Edit / Delete cell and an optional totals row; phones show the same rows as a List of Row, each with its own Delete icon button. Add, Edit or a row press opens the RecordForm in a Drawer; Delete asks first. Empty = "No records yet", an undeclared kind = the unknown state, loading = a Skeleton.`,
  status: {
    web: { state: `ok`, symbol: `RecordTable (macro)`, file: `packages/exponential-ui/catalog/macros.json`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/record_table.rs` },
    ios: { state: `ok`, symbol: `RecordTable (macro)`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `RecordTable (macro)`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-record-table" />,
}
