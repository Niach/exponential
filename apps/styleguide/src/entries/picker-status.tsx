import { placeholderMarkup } from "./placeholder.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1029 contract — PLACEHOLDER. EXP-1021 fills this entry (the body, the
// status table) and drops `placeholder`; it never edits `index.ts` or the
// section index.
export const entry: StyleguideEntry = {
  id: `picker-status`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Status picker`,
  blurb: `The team's statuses in display order, each glyph in its colour.`,
  placeholder: true,
  render: () => placeholderMarkup({ owner: `EXP-1021`, title: `Status picker` }),
}
