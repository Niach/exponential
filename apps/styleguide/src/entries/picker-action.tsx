import { ActionPicker, PickerList, PickerTrigger, actionPickerItems } from "@exp/ui"

import { PickerSpecimen, typedPickerStatus } from "./picker-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

const noop = (): void => {}

/* An action (EXP-1293: a saved prompt) row is its curated icon plus its
   name; one without an icon of its own draws the default action glyph. */
const ACTIONS = [
  { id: `release`, name: `Cut a release`, icon: `rocket` },
  { id: `triage`, name: `Triage the inbox`, icon: `inbox` },
  { id: `changelog`, name: `Write the changelog` },
]

export const entry: StyleguideEntry = {
  id: `picker-action`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Action picker`,
  blurb: `Rows by curated icon and name; a row with no icon of its own draws the default glyph rather than nothing. NEW-2: prompts replaced actions and no app screen opens this picker any more; it stays for the natives' mirrors until NEW-8.`,
  status: typedPickerStatus(`ActionPicker`, {
    web: `action-picker.tsx`,
    desktop: `action_picker.rs`,
    ios: `ActionPicker.swift`,
    android: `ActionPicker.kt`,
  }),
  island: () => (
    <PickerSpecimen
      trigger={
        <ActionPicker
          actions={ACTIONS}
          value="release"
          onChange={noop}
          trigger={
            <PickerTrigger variant="row" label="Action" value="Cut a release" />
          }
        />
      }
      surface={
        <PickerList
          mode="single"
          search
          searchPlaceholder="Search actions…"
          items={actionPickerItems(ACTIONS)}
          value="release"
          onChange={noop}
        />
      }
    />
  ),
}
