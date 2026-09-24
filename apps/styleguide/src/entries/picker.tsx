import { placeholderMarkup } from "./placeholder.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1029 contract — PLACEHOLDER. EXP-1021 fills this entry (the body, the
// status table) and drops `placeholder`; it never edits `index.ts` or the
// section index.
export const entry: StyleguideEntry = {
  id: `picker`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Picker`,
  blurb: `THE picker primitive: one surface per platform (a popover at the trigger on a pointer, a bottom sheet of plain rows on a phone), single or multi, optional search.`,
  placeholder: true,
  render: () => placeholderMarkup({ owner: `EXP-1021`, title: `Picker` }),
}
