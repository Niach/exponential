import { placeholderMarkup } from "./placeholder.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1029 contract — PLACEHOLDER. EXP-1031 fills this entry (the body, the
// status table) and drops `placeholder`; it never edits `index.ts` or the
// section index.
export const entry: StyleguideEntry = {
  id: `toast`,
  section: `general`,
  owner: `EXP-1031`,
  title: `Toast`,
  blurb: `Transient notices and alerts. Placeholder only; EXP-1031 fills it.`,
  placeholder: true,
  render: () => placeholderMarkup({ owner: `EXP-1031`, title: `Toast` }),
}
