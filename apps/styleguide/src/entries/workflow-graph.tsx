import { placeholderMarkup } from "./placeholder.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1029 contract — PLACEHOLDER. EXP-1014 fills this entry (the body, the
// status table) and drops `placeholder`; it never edits `index.ts` or the
// section index.
export const entry: StyleguideEntry = {
  id: `workflow-graph`,
  section: `views`,
  owner: `EXP-1014`,
  title: `Workflow graph`,
  blurb: `The workflow screen: nodes by wave and lane, the node panel, no settings panel.`,
  placeholder: true,
  render: () => placeholderMarkup({ owner: `EXP-1014`, title: `Workflow graph` }),
}
