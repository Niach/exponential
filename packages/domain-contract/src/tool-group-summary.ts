// EXP-785: the ONE caption a collapsed tool group renders, derived from the
// group's tool rows. Hand-mirrored ×4 (Android `domain/ToolGroupSummary.kt`,
// iOS `ExpCore/Sources/Domain/ToolGroupSummary.swift`, desktop
// `steer::tool_group_summary`) and byte-locked by
// `fixtures/tool-group-summary.json`, which every client's test runs.
//
// Rules:
// - `kind` is a contract `toolKind` value (`read`, `edit`, `delete`, `move`,
//   `search`, `execute`, `think`, `fetch`, `switch_mode`, `other`); an unknown
//   kind counts as `other` so an older client never throws on a newer agent.
// - `edit`/`delete`/`move` are "edited N files", `read` is "read N files",
//   each DEDUPED by `detail` (the path). A null/empty detail is its own
//   distinct file every time.
// - Fixed segment order: ran N commands · edited N files · read N files ·
//   searched N times · fetched N pages · N other tools · N failed. Zero
//   segments are omitted; `failed` counts calls with `failed: true` of ANY
//   kind and is always last.
// - Only the first character of the whole caption is capitalised.
// - Every call `think`/`switch_mode`/`other` → `Used N tools` (plus
//   ` · N failed`); no calls at all → `No tool calls`.

export interface ToolCallSummary {
  kind: string
  detail?: string | null
  failed: boolean
}

/** The segment separator: space, MIDDLE DOT (U+00B7), space. */
export const TOOL_GROUP_SUMMARY_SEPARATOR = ` · `

function count(n: number, singular: string, plural = `${singular}s`): string {
  return `${n} ${n === 1 ? singular : plural}`
}

export function toolGroupSummary(calls: readonly ToolCallSummary[]): string {
  if (calls.length === 0) return `No tool calls`
  let commands = 0
  let searches = 0
  let fetches = 0
  let other = 0
  let failed = 0
  const edited = new Set<string>()
  let editedBlank = 0
  const read = new Set<string>()
  let readBlank = 0
  for (const call of calls) {
    if (call.failed) failed += 1
    switch (call.kind) {
      case `execute`:
        commands += 1
        break
      case `edit`:
      case `delete`:
      case `move`:
        if (call.detail) edited.add(call.detail)
        else editedBlank += 1
        break
      case `read`:
        if (call.detail) read.add(call.detail)
        else readBlank += 1
        break
      case `search`:
        searches += 1
        break
      case `fetch`:
        fetches += 1
        break
      default:
        other += 1
        break
    }
  }
  const editedCount = edited.size + editedBlank
  const readCount = read.size + readBlank
  const segments: string[] = []
  if (commands > 0) segments.push(`ran ${count(commands, `command`)}`)
  if (editedCount > 0) segments.push(`edited ${count(editedCount, `file`)}`)
  if (readCount > 0) segments.push(`read ${count(readCount, `file`)}`)
  if (searches > 0) segments.push(`searched ${count(searches, `time`)}`)
  if (fetches > 0) segments.push(`fetched ${count(fetches, `page`)}`)
  if (segments.length === 0) {
    segments.push(`used ${count(other, `tool`)}`)
  } else if (other > 0) {
    segments.push(count(other, `other tool`))
  }
  if (failed > 0) segments.push(`${failed} failed`)
  const text = segments.join(TOOL_GROUP_SUMMARY_SEPARATOR)
  return text.charAt(0).toUpperCase() + text.slice(1)
}
