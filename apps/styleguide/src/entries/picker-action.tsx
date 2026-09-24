import { ActionPicker, PickerList, PickerTrigger, actionPickerItems } from "@exp/ui"

import { PickerSpecimen, typedPickerStatus } from "./picker-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

const noop = (): void => {}

/* An action row is its curated icon plus its name. An action with a required
   input cannot be AUTOMATED — nobody is there to fill it in — so the
   automation editor renders that row and disables it rather than hiding it. */
const ACTIONS = [
  { id: `release`, name: `Cut a release`, icon: `rocket` },
  { id: `triage`, name: `Triage the inbox`, icon: `inbox` },
  { id: `conflicts`, name: `Fix merge conflicts`, icon: `git-merge`, disabled: true },
]

export const entry: StyleguideEntry = {
  id: `picker-action`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Action picker`,
  blurb: `The team's actions (and the two listed builtins) by curated icon and name — the Agent composer's action chip and the automation editor's "Action" row. An action whose inputs are required is rendered and disabled in the automation arm, because an automated run has nobody to fill them in; a row with no icon of its own draws the default action glyph rather than nothing.`,
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
