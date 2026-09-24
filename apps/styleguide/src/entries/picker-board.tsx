import { placeholderMarkup } from "./placeholder.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1029 contract — PLACEHOLDER. EXP-1021 fills this entry (the body, the
// status table) and drops `placeholder`; it never edits `index.ts` or the
// section index.
export const entry: StyleguideEntry = {
  id: `picker-board`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Board picker`,
  blurb: `Boards by icon and colour, everywhere a board is picked.`,
  placeholder: true,
  render: () => placeholderMarkup({ owner: `EXP-1021`, title: `Board picker` }),
}
