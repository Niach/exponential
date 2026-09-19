// EXP-988 contract: what an Exponential MCP tool RESULT points at, so a run
// transcript can render a tool row as pills / hover cards instead of raw JSON
// (owner: EXP-920, which also owns every preview, badge and hover-card
// component, on all platforms and in the styleguide).
//
// `toolResultPreview` is pure: it reads the tool's parsed result (the JSON the
// `ok()` helper serialized) and returns the entities worth a pill. `href` is
// the in-app route (`/t/$teamSlug/boards/$boardSlug/issues/$id` and friends)
// so a click lands where the entity lives; `title` is the display string the
// result already carries (an issue's identifier + title, a board's name), or
// absent when the result names only an id and the client resolves it from its
// synced rows. A list result yields ONE `list` ref (count in `title`) plus at
// most a handful of member refs; a `pr_*` result refs the ISSUES the PR lands
// on. preview.test.ts lists EVERY registered tool with the kinds its result
// yields, so a tool added later fails there until it is mapped.
export const entityRefKinds = [
  `issue`,
  `board`,
  `action`,
  `automation`,
  `comment`,
  `session`,
  `label`,
  `status`,
  `list`,
] as const
export type EntityRefKind = (typeof entityRefKinds)[number]

export interface EntityRef {
  kind: EntityRefKind
  id: string
  title?: string
  href: string
}

export function toolResultPreview(
  _toolName: string,
  _result: unknown
): EntityRef[] {
  throw new Error(`toolResultPreview is not implemented yet (EXP-920 owns it)`)
}
