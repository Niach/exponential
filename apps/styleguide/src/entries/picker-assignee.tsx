import { placeholderMarkup } from "./placeholder.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1029 contract — PLACEHOLDER. EXP-1021 fills this entry (the body, the
// status table) and drops `placeholder`; it never edits `index.ts` or the
// section index.
export const entry: StyleguideEntry = {
  id: `picker-assignee`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Assignee picker`,
  blurb: `Teammates by avatar, name and email, Unassigned first.`,
  placeholder: true,
  render: () => placeholderMarkup({ owner: `EXP-1021`, title: `Assignee picker` }),
}
