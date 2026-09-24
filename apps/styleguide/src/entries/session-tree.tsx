import { placeholderMarkup } from "./placeholder.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1029 contract — PLACEHOLDER. EXP-996 fills this entry (the body, the
// status table) and drops `placeholder`; it never edits `index.ts` or the
// section index.
export const entry: StyleguideEntry = {
  id: `session-tree`,
  section: `special`,
  owner: `EXP-996`,
  title: `Session tree`,
  blurb: `Runs nested under their parent, workflow and stack groups, resumes collapsed.`,
  placeholder: true,
  render: () => placeholderMarkup({ owner: `EXP-996`, title: `Session tree` }),
}
