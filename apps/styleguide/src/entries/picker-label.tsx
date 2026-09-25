import { LabelPicker, PickerList, PickerTrigger, labelPickerItems } from "@exp/ui"

import { PickerSpecimen, typedPickerStatus } from "./picker-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

const noop = (): void => {}

const LABELS = [
  { id: `bug`, name: `bug`, color: `#ef4444` },
  { id: `mobile`, name: `mobile`, color: `#3b82f6` },
  { id: `design`, name: `design`, color: `#a855f7` },
  { id: `docs`, name: `docs`, color: `#22c55e` },
]

export const entry: StyleguideEntry = {
  id: `picker-label`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Label picker`,
  blurb: `ALWAYS multi, always searchable, each row its colour dot and name — the issue properties, the create dialog, the context menu, the bulk bar, the widget's default labels and the automation trigger's label filter. This is the picker the multi selection language was designed on: the surface stays open across toggles (a batch is several picks) and every picked row reads as the row's OWN highlight, never a leading circle or a checkbox. Two primitive slots carry the rest: "Create label" is the \`footer\`, and the create form is the \`panel\` that replaces the search field and the rows while it is up.`,
  status: typedPickerStatus(`LabelPicker`, {
    web: `label-picker.tsx`,
    desktop: `label_picker.rs`,
    ios: `LabelPicker.swift`,
    android: `LabelPicker.kt`,
  }),
  island: () => (
    <PickerSpecimen
      trigger={
        <LabelPicker
          labels={LABELS}
          value={[`bug`, `mobile`]}
          onChange={noop}
          trigger={
            <PickerTrigger variant="pill" label="Labels" value="bug, mobile" />
          }
        />
      }
      surface={
        <PickerList
          mode="multi"
          search
          searchPlaceholder="Filter labels…"
          items={labelPickerItems(LABELS)}
          value={[`bug`, `mobile`]}
          onChange={noop}
        />
      }
      caption="A bulk edit adds a third state: a label on SOME of the selection is marked `indeterminate`."
    />
  ),
}
