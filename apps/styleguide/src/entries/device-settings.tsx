import { placeholderMarkup } from "./placeholder.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1029 contract — PLACEHOLDER. EXP-1020 fills this entry (the body, the
// status table) and drops `placeholder`; it never edits `index.ts` or the
// section index.
export const entry: StyleguideEntry = {
  id: `device-settings`,
  section: `special`,
  owner: `EXP-1020`,
  title: `Device settings`,
  blurb: `The per-machine agent defaults: account, models, workflow settings sub-shell.`,
  placeholder: true,
  render: () => placeholderMarkup({ owner: `EXP-1020`, title: `Device settings` }),
}
