import { placeholderMarkup } from "./placeholder.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1029 contract — PLACEHOLDER. EXP-1021 fills this entry (the body, the
// status table) and drops `placeholder`; it never edits `index.ts` or the
// section index.
export const entry: StyleguideEntry = {
  id: `picker-priority`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Priority picker`,
  blurb: `The priorities in contract order, each glyph in its tone.`,
  placeholder: true,
  render: () => placeholderMarkup({ owner: `EXP-1021`, title: `Priority picker` }),
}
