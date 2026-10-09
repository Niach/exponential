import {
  SESSION_RESULT_FILES_MAX,
  SESSION_RESULTS_FILES_TOTAL_MAX,
  SESSION_RESULTS_MAX,
  SESSION_RESULTS_REPORT_TOTAL_MAX,
  type CodingSessionResult,
} from "@exp/db-schema/domain"

// EXP-879: the SERVER's half of `coding_sessions.results` — the pure list
// algebra the token-gated upload route and MCP `exponential_sessions_guide`
// (alias `exponential_sessions_results`, EXP-1251) run inside their `FOR UPDATE` transaction. Kept out of both so the ordering
// rules (replace in place, append at the end, delete a label or a whole topic)
// are testable without a database.
//
// `(topic, label)` is the upsert key: re-publishing a picture REPLACES the one
// already filed under that pair, keeping its position — an agent iterating on
// one screen must not push the rest of its topic around. The displaced
// attachment id comes back so the caller can delete that row and its S3
// object; nothing here touches storage.

export interface SessionResultUpsert {
  results: CodingSessionResult[]
  /** The attachment the new picture replaced, if any — the caller deletes its
   *  row and blob AFTER the transaction commits. */
  displacedAttachmentId: string | null
}

export function upsertSessionResult(
  current: CodingSessionResult[] | null,
  entry: CodingSessionResult
): SessionResultUpsert {
  const results = [...(current ?? [])]
  const index = results.findIndex(
    (row) => !isTextEntry(row) && row?.topic === entry.topic && row?.label === entry.label
  )
  if (index === -1) {
    results.push(entry)
    return { results, displacedAttachmentId: null }
  }
  const displaced = results[index]?.attachmentId ?? null
  results[index] = entry
  return {
    results,
    // A replay that lands the SAME attachment id must not delete the object it
    // just wrote.
    displacedAttachmentId:
      displaced && displaced !== entry.attachmentId ? displaced : null,
  }
}

export interface SessionResultRemoval {
  results: CodingSessionResult[]
  /** Every attachment the removal orphaned — rows and blobs to reclaim. */
  removedAttachmentIds: string[]
}

/** EXP-933: a text entry = the topic's report (label + attachmentId null). */
export function isTextEntry(row: CodingSessionResult | null | undefined): boolean {
  return !!row && typeof row.text === `string` && !row.attachmentId
}

/** Sum of every text entry's length, the row-level report cap's measure. */
export function reportTextLength(results: readonly CodingSessionResult[]): number {
  let total = 0
  for (const row of results) if (isTextEntry(row)) total += row.text?.length ?? 0
  return total
}

/** EXP-1154: every text entry's `files` count, the run-level files cap. */
export function reportFileCount(results: readonly CodingSessionResult[]): number {
  let total = 0
  for (const row of results) {
    if (isTextEntry(row) && Array.isArray(row.files)) total += row.files.length
  }
  return total
}

/** EXP-1154: a topic's files as stored: strings only, trimmed, blanks and
 *  duplicates dropped (first position kept), capped per topic. */
export function cleanSessionResultFiles(files: readonly unknown[]): string[] {
  const out: string[] = []
  const seen = new Set<string>()
  for (const raw of files) {
    if (typeof raw !== `string`) continue
    const path = raw.trim()
    if (!path || seen.has(path)) continue
    seen.add(path)
    out.push(path)
    if (out.length >= SESSION_RESULT_FILES_MAX) break
  }
  return out
}

/**
 * EXP-933: file (or replace) a topic's report text. One text entry per topic:
 * replaced IN PLACE, else appended, so an agent that writes its `Summary`
 * first gets it at the top. A new topic's text therefore opens that topic;
 * pictures filed later under it join the same group. Returns null when the
 * list would outgrow the count cap, the report-total cap or the files cap.
 *
 * EXP-1154 `files`: `undefined` keeps the replaced entry's files, `[]` clears
 * them, anything else is cleaned (`cleanSessionResultFiles`). The total caps
 * refuse only a write that GROWS past them, so a run filed under the older,
 * larger caps can still shorten its report.
 *
 * EXP-1245/1251 `meta`: `at` = the server's write stamp; `prUrl` scopes the
 * topic to one PR (`undefined` keeps the replaced entry's tag, null or blank
 * clears it).
 */
export function upsertSessionResultText(
  current: CodingSessionResult[] | null,
  topic: string,
  text: string,
  files?: readonly string[],
  meta: { at?: number; prUrl?: string | null } = {}
): CodingSessionResult[] | null {
  const before = current ?? []
  const results = [...before]
  const index = results.findIndex((row) => isTextEntry(row) && row.topic === topic)
  const kept =
    files === undefined
      ? index === -1 || !Array.isArray(results[index]?.files)
        ? []
        : cleanSessionResultFiles(results[index]!.files!)
      : cleanSessionResultFiles(files)
  const prUrl =
    meta.prUrl === undefined
      ? index === -1
        ? null
        : (results[index]?.prUrl ?? null)
      : meta.prUrl?.trim() || null
  const entry: CodingSessionResult = {
    topic,
    label: null,
    attachmentId: null,
    width: null,
    height: null,
    text,
    ...(kept.length ? { files: kept } : {}),
    ...(prUrl ? { prUrl } : {}),
    ...(meta.at !== undefined ? { at: meta.at } : {}),
  }
  if (index === -1) {
    // A topic that already has pictures gets its text at the topic's FIRST
    // position, so the report reads above its tiles in every reader.
    const first = results.findIndex((row) => row?.topic === topic)
    if (first === -1) results.push(entry)
    else results.splice(first, 0, entry)
  } else {
    results[index] = entry
  }
  if (exceedsSessionResultsCap(results)) return null
  const textTotal = reportTextLength(results)
  if (
    textTotal > SESSION_RESULTS_REPORT_TOTAL_MAX &&
    textTotal > reportTextLength(before)
  ) {
    return null
  }
  const fileTotal = reportFileCount(results)
  if (fileTotal > SESSION_RESULTS_FILES_TOTAL_MAX && fileTotal > reportFileCount(before)) {
    return null
  }
  return results
}

/** `label` removes one picture; without it the WHOLE topic goes (its text
 *  too). `text: true` removes only the topic's report text. */
export function removeSessionResults(
  current: CodingSessionResult[] | null,
  target: { topic: string; label?: string | null; text?: boolean }
): SessionResultRemoval {
  const results: CodingSessionResult[] = []
  const removedAttachmentIds: string[] = []
  for (const row of current ?? []) {
    const hit =
      row?.topic === target.topic &&
      (target.text
        ? isTextEntry(row)
        : target.label == null || (!isTextEntry(row) && row?.label === target.label))
    if (hit) {
      if (typeof row?.attachmentId === `string` && row.attachmentId) {
        removedAttachmentIds.push(row.attachmentId)
      }
      continue
    }
    results.push(row)
  }
  return { results, removedAttachmentIds }
}

/** The compact list EVERY sessions_results response carries: what the run has
 *  published so far, without the ids or the pixel sizes an agent cannot use. */
export function resultsSummary(
  results: CodingSessionResult[] | null
): Array<{ topic: string; label: string } | { topic: string; text: number }> {
  const out: Array<{ topic: string; label: string } | { topic: string; text: number }> = []
  for (const row of results ?? []) {
    if (typeof row?.topic !== `string`) continue
    // A text entry reports its length: enough to confirm it landed.
    if (isTextEntry(row)) out.push({ topic: row.topic, text: row.text?.length ?? 0 })
    else if (typeof row.label === `string`) out.push({ topic: row.topic, label: row.label })
  }
  return out
}

/** True once a list would outgrow the column's cap (the route answers 409). */
export function exceedsSessionResultsCap(
  results: readonly CodingSessionResult[]
): boolean {
  return results.length > SESSION_RESULTS_MAX
}

/** EXP-1172: the label a `sessions_show` picture filed without one gets —
 *  `Shot N`, N one past the topic's pictures, bumped past any taken label so
 *  a show never REPLACES an earlier picture by accident. Picked under the
 *  upload's row lock, so two shows in flight never collide. */
export function nextShowLabel(
  results: readonly CodingSessionResult[] | null,
  topic: string
): string {
  const taken = new Set<string>()
  for (const row of results ?? []) {
    if (row?.topic === topic && !isTextEntry(row) && typeof row.label === `string`) {
      taken.add(row.label)
    }
  }
  let n = taken.size + 1
  while (taken.has(`Shot ${n}`)) n += 1
  return `Shot ${n}`
}

/** EXP-1251: the listed paths a branch/PR diff does not have: a path matches
 *  a diff file's `path` or its rename source (`previousPath`), the Guide's
 *  coverage rule (`guidePathMatches`, @exp/ui). */
export function missingGuideFiles(
  listed: readonly string[],
  diffFiles: readonly { path: string; previousPath?: string | null }[]
): string[] {
  return listed.filter(
    (path) =>
      !diffFiles.some(
        (file) => file.path === path || (!!file.previousPath && file.previousPath === path)
      )
  )
}

/** EXP-1251: when a run opens a NEW PR, the text topics filed so far without
 *  a `prUrl` belong to it: they are tagged, so a later stacked PR's body and
 *  issue page never claim them. Tagged topics keep their tag. */
export function stampUntaggedResults(
  results: readonly CodingSessionResult[] | null,
  prUrl: string
): { results: CodingSessionResult[]; changed: boolean } {
  let changed = false
  const next = (results ?? []).map((row) => {
    if (!isTextEntry(row) || (typeof row.prUrl === `string` && row.prUrl.trim())) return row
    changed = true
    return { ...row, prUrl }
  })
  return { results: next, changed }
}
