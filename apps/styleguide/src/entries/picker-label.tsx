import { placeholderMarkup } from "./placeholder.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1029 contract — PLACEHOLDER. EXP-1021 fills this entry (the body, the
// status table) and drops `placeholder`; it never edits `index.ts` or the
// section index.
export const entry: StyleguideEntry = {
  id: `picker-label`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Label picker`,
  blurb: `Always multi, searchable, colour dots; the surface stays open across toggles.`,
  placeholder: true,
  render: () => placeholderMarkup({ owner: `EXP-1021`, title: `Label picker` }),
}
