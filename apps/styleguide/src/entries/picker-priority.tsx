import { PickerList, PickerTrigger, PriorityPicker, conceptIcon, priorityPickerItems } from "@exp/ui"

import { PickerSpecimen, typedPickerStatus } from "./picker-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

const noop = (): void => {}

/* The contract's `issuePriority` in its display order (REV2-85) — the table
   itself lives in the app beside the enum, the picker never owns it. */
const PRIORITIES = [
  { value: `urgent`, label: `Urgent`, icon: conceptIcon(`priority-urgent`), color: `text-red-500` },
  { value: `high`, label: `High`, icon: conceptIcon(`priority-high`), color: `text-orange-500` },
  { value: `medium`, label: `Medium`, icon: conceptIcon(`priority-medium`), color: `text-yellow-500` },
  { value: `low`, label: `Low`, icon: conceptIcon(`priority-low`), color: `text-blue-500` },
  { value: `none`, label: `No priority`, icon: conceptIcon(`priority-none`), color: `text-muted-foreground` },
]

export const entry: StyleguideEntry = {
  id: `picker-priority`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Priority picker`,
  blurb: `The contract's five priorities in display order, each by its glyph in its tone — the issue's priority chip and button, the create dialog, the phone properties row, the context menu, the bulk bar and the automation trigger's priority filter (the same picker, multi, capped). No search: five fixed rows are a menu, not a list. The TABLE is the app's, beside the enum — the picker is handed rows and never owns the vocabulary, which is why the trigger draws the RESOLVED config rather than a matched row (an unknown forward-compat value falls back instead of rendering blank).`,
  status: typedPickerStatus(`PriorityPicker`, {
    web: `priority-picker.tsx`,
    desktop: `priority_picker.rs`,
    ios: `PriorityPicker.swift`,
    android: `PriorityPicker.kt`,
  }),
  island: () => (
    <PickerSpecimen
      trigger={
        <PriorityPicker
          options={PRIORITIES}
          value="high"
          onChange={noop}
          trigger={<PickerTrigger variant="pill" label="Priority" value="High" />}
        />
      }
      surface={
        <PickerList
          mode="single"
          items={priorityPickerItems(PRIORITIES)}
          value="high"
          onChange={noop}
        />
      }
    />
  ),
}
