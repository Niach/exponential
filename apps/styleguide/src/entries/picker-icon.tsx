import { placeholderMarkup } from "./placeholder.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1029 contract — PLACEHOLDER. EXP-1021 fills this entry (the body, the
// status table) and drops `placeholder`; it never edits `index.ts` or the
// section index.
export const entry: StyleguideEntry = {
  id: `picker-icon`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Icon picker`,
  blurb: `'IconPicker(set)': the board or the device glyph set over the swatch grid.`,
  placeholder: true,
  render: () => placeholderMarkup({ owner: `EXP-1021`, title: `Icon picker` }),
}
