// EXP-988 contract: the resume chain of a coding session (owner: EXP-974,
// which also owns the merged issue/run face toggle; EXP-902 must NOT touch the
// toggle — it only changes which face a tab opens on, reading
// `startedReason === 'agent'` / `parentSessionId`, kept across resumes since
// EXP-906).
//
// A resume is the SAME run under a new row (`resumed_from_id` → predecessor,
// EXP-637/EXP-906), so the Run face shows the whole succession as one thing.
// `runChain` is a pure SELECTOR over the synced `coding_sessions` rows: it
// follows `resumedFromId` BACKWARDS to the first row and FORWARDS to the
// latest, and returns the chain oldest-first, the named session included.
// An unknown id yields []. A fork (two rows resuming the same predecessor)
// follows the NEWEST successor by `createdAt`; the others are separate chains
// of their own.
import type { CodingSession } from "@/db/schema"

export type SessionRow = Pick<
  CodingSession,
  `id` | `resumedFromId` | `createdAt`
>

export function runChain<T extends SessionRow>(
  _rows: readonly T[],
  _sessionId: string
): T[] {
  throw new Error(`runChain is not implemented yet (EXP-974 owns it)`)
}
