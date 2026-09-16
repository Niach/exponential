import {
  SESSION_RESULTS_MAX,
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
    (row) => row?.topic === entry.topic && row?.label === entry.label
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

/** `label` removes one picture; without it the WHOLE topic goes. */
export function removeSessionResults(
  current: CodingSessionResult[] | null,
  target: { topic: string; label?: string | null }
): SessionResultRemoval {
  const results: CodingSessionResult[] = []
  const removedAttachmentIds: string[] = []
  for (const row of current ?? []) {
    const hit =
      row?.topic === target.topic &&
      (target.label == null || row?.label === target.label)
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
): Array<{ topic: string; label: string }> {
  return (results ?? [])
    .filter((row) => typeof row?.topic === `string` && typeof row?.label === `string`)
    .map((row) => ({ topic: row.topic, label: row.label }))
}

/** True once a list would outgrow the column's cap (the route answers 409). */
export function exceedsSessionResultsCap(
  results: readonly CodingSessionResult[]
): boolean {
  return results.length > SESSION_RESULTS_MAX
}
