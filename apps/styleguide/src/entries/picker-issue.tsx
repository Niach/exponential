import { IssuePicker, PickerList, PickerTrigger, issuePickerItems } from "@exp/ui"

import { PickerSpecimen, typedPickerStatus } from "./picker-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

const noop = (): void => {}

/* The row reads `IDENT Title` and search matches either half — two issues may
   share a title, so `value` is the id and the identifier is a keyword. */
const ISSUES = [
  { id: `i1`, identifier: `EXP-1021`, title: `Consistent bottom sheet pickers` },
  { id: `i2`, identifier: `EXP-1029`, title: `The shared picker API` },
  { id: `i3`, identifier: `EXP-941`, title: `One searchable picker on the web` },
]

export const entry: StyleguideEntry = {
  id: `picker-issue`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Issue picker`,
  blurb: `Issues by identifier and title, single or multi. Single is the relations linker (sub-issue, blocks, duplicate — the surface this whole issue was measured against), the mark-as-duplicate flow and the stack dialog; multi is the composer picking a batch. Over a big board the ranking is the shared engine's (\`useIssueSearchResults\`, EXP-892) and the picker renders what it is handed, verbatim — so \`shouldFilter\` goes off and the top row stays selected as results arrive.`,
  status: typedPickerStatus(`IssuePicker`, {
    web: `issue-picker.tsx`,
    desktop: `issue_picker.rs`,
    ios: `IssuePicker.swift`,
    android: `IssuePicker.kt`,
  }),
  island: () => (
    <PickerSpecimen
      trigger={
        <IssuePicker
          issues={ISSUES}
          value={null}
          onChange={noop}
          trigger={
            <PickerTrigger variant="pill" label="Issue" placeholder="Link an issue" />
          }
        />
      }
      surface={
        <PickerList
          mode="multi"
          search
          searchPlaceholder="Search issues…"
          items={issuePickerItems(ISSUES)}
          value={[`i1`]}
          onChange={noop}
        />
      }
      caption="Multi (a batch): the picked rows are highlighted and the surface stays open."
    />
  ),
}
