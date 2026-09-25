import { placeholderMarkup } from "./placeholder.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1029 contract — PLACEHOLDER. EXP-1019 fills this entry (the body, the
// status table) and drops `placeholder`; it never edits `index.ts` or the
// section index.
export const entry: StyleguideEntry = {
  id: `composer-dialog`,
  section: `special`,
  owner: `EXP-1019`,
  title: `Composer dialog`,
  blurb: `The one launcher: issue or action chips, free text, account, device, model and effort.`,
  placeholder: true,
  render: () => placeholderMarkup({ owner: `EXP-1019`, title: `Composer dialog` }),
}
