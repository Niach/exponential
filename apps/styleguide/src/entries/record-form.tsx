import { ExponentialUiSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// VAPP-217 (eApps U2) — Exponential UI's `RecordForm` macro, painted by the
// real React renderer from fixtures/specimens.json `exponential-ui-record-form`
// (the catalog example, host-filled props included: src/host/record-macros.ts).

export const entry: StyleguideEntry = {
  id: `record-form`,
  section: `general`,
  owner: `VAPP-217`,
  title: `Record form`,
  blurb: `A form over one record kind of an app: Exponential UI's RecordForm macro. The host fills one field per schema field (required first, then by name): date → DatePicker, time → TimePicker, number/integer → NumberField, boolean → Switch, enum → Select, member / issue ref → a searchable Select, string list → ChipInput, long text → Textarea, else Input; checks come from the schema (required, maxLength, minimum/maximum). Short fields pair in a two-column FieldGroup on wide frames; long ones span both. Submit = records.upsert; readOnly disables the form and drops the submit.`,
  status: {
    web: { state: `ok`, symbol: `RecordForm (macro)`, file: `packages/exponential-ui/catalog/macros.json`, note: `here: ExponentialUiSpecimen (packages/ui)` },
    desktop: { state: `ok`, symbol: `render`, file: `apps/desktop/crates/ui/src/styleguide/entries/record_form.rs` },
    ios: { state: `ok`, symbol: `RecordForm (macro)`, file: `packages/exponential-ui-swift/Sources/ExponentialUI` },
    android: { state: `ok`, symbol: `RecordForm (macro)`, file: `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose` },
  },
  island: () => <ExponentialUiSpecimen id="exponential-ui-record-form" />,
}
