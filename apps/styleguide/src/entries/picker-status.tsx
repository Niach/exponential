import { PickerList, PickerTrigger, StatusPicker, conceptIcon, statusPickerItems } from "@exp/ui"

import { PickerSpecimen, typedPickerStatus } from "./picker-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

const noop = (): void => {}

/* A team's six locked builtins plus its own rows (EXP-314), in the contract's
   display order. The colour slot carries the RESOLVED colour either way — a
   builtin's token class or a custom row's hex — so neither the picker nor the
   app needs a second colour table. */
const STATUSES = [
  {
    id: `backlog`,
    name: `Backlog`,
    category: `backlog`,
    icon: conceptIcon(`status-backlog`),
    colorHex: `text-muted-foreground`,
  },
  {
    id: `in-progress`,
    name: `In Progress`,
    category: `started`,
    icon: conceptIcon(`status-in-progress`),
    colorHex: `text-yellow-500`,
  },
  {
    id: `in-review`,
    name: `In Review`,
    category: `started`,
    icon: conceptIcon(`status-in-review`),
    colorHex: `text-blue-500`,
  },
  {
    id: `staging`,
    name: `On staging`,
    category: `started`,
    icon: conceptIcon(`status-in-progress`),
    colorHex: `#a855f7`,
  },
  {
    id: `done`,
    name: `Done`,
    category: `completed`,
    icon: conceptIcon(`status-done`),
    colorHex: `text-green-500`,
  },
]

export const entry: StyleguideEntry = {
  id: `picker-status`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Status picker`,
  blurb: `The team's own \`issue_statuses\` rows (EXP-314) in display order, each by its glyph in its colour — the issue header chip, the create dialog, the phone properties row, the list row's context menu, the bulk bar and the PR-automation target. Six builtins are LOCKED (never renamed, recoloured or deleted) and a custom row is name + colour; the picker takes the RESOLVED row, so the builtin's token class and the custom's hex reach it through one slot. The duplicate CATEGORY is never offered here: picking it opens the canonical-issue picker instead.`,
  status: typedPickerStatus(`StatusPicker`, {
    web: `status-picker.tsx`,
    desktop: `status_picker.rs`,
    ios: `StatusPicker.swift`,
    android: `StatusPicker.kt`,
  }),
  island: () => (
    <PickerSpecimen
      trigger={
        <StatusPicker
          statuses={STATUSES}
          value="in-progress"
          onChange={noop}
          trigger={
            <PickerTrigger variant="pill" label="Status" value="In Progress" />
          }
        />
      }
      surface={
        <PickerList
          mode="single"
          items={statusPickerItems(STATUSES)}
          value="in-progress"
          onChange={noop}
        />
      }
      caption="The last row is a CUSTOM status: its hex tints the glyph exactly as a builtin's token does."
    />
  ),
}
