import {
  SESSION_RESULTS_MAX,
  SESSION_RESULTS_REPORT_TOTAL_MAX,
  type CodingSessionResult,
} from "@exp/db-schema/domain"

// EXP-879: the SERVER's half of `coding_sessions.results` — the pure list
// algebra the token-gated upload route and MCP `exponential_sessions_results`
// run inside their `FOR UPDATE` transaction. Kept out of both so the ordering
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

/**
 * EXP-933: file (or replace) a topic's report text. One text entry per topic:
 * replaced IN PLACE, else appended, so an agent that writes its `Summary`
 * first gets it at the top. A new topic's text therefore opens that topic;
 * pictures filed later under it join the same group. Returns null when the
 * list would outgrow the count cap or the report-total cap.
 */
export function upsertSessionResultText(
  current: CodingSessionResult[] | null,
  topic: string,
  text: string
): CodingSessionResult[] | null {
  const results = [...(current ?? [])]
  const entry: CodingSessionResult = {
    topic,
    label: null,
    attachmentId: null,
    width: null,
    height: null,
    text,
  }
  const index = results.findIndex((row) => isTextEntry(row) && row.topic === topic)
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
  if (reportTextLength(results) > SESSION_RESULTS_REPORT_TOTAL_MAX) return null
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
