import { placeholderMarkup } from "./placeholder.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1029 contract — PLACEHOLDER. EXP-1020 fills this entry (the body, the
// status table) and drops `placeholder`; it never edits `index.ts` or the
// section index.
export const entry: StyleguideEntry = {
  id: `sub-shell`,
  section: `general`,
  owner: `EXP-1020`,
  title: `Sub-shell navigation`,
  blurb: `A settings row that slides a child page in place of the whole card, back button on top.`,
  placeholder: true,
  render: () => placeholderMarkup({ owner: `EXP-1020`, title: `Sub-shell navigation` }),
}
