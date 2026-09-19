// EXP-988 contract for `exponential_sessions_compact` (owner: EXP-936).
//
// Compaction is a REQUEST, never an act: the server relays a `compact_request`
// frame (steer protocol, `ServerFrame`) to the run's publisher and the HOST
// executes it at the next turn boundary. Guardrails (open decision 2, the
// planner's reading of an issue text that breaks off mid-sentence):
//
//  - only the CALLING session (the `X-Exp-Session-Id` run, owner or host);
//    anything else is `not_own_session`;
//  - refused below 50 % context use (`too_early`) and within 20 turns of the
//    last compaction (`cooldown`) — the device knows both, the server only
//    carries the verdict back;
//  - `unsupported_agent` for a run whose agent has no compaction command;
//  - `keep` is carried verbatim into the compaction prompt (what the summary
//    must preserve), `reason` is logged on the run's feed.
import { NotImplementedError } from "./not-implemented"

export const sessionsCompactRefusals = [
  `too_early`,
  `cooldown`,
  `not_own_session`,
  `unsupported_agent`,
] as const
export type SessionsCompactRefusal = (typeof sessionsCompactRefusals)[number]

export interface SessionsCompactInput {
  /** The header run (the caller's own session). */
  sessionId: string
  userId: string
  reason: string
  keep?: string
}

export interface SessionsCompactResult {
  accepted: boolean
  refusedBecause?: SessionsCompactRefusal
}

export async function requestSessionCompaction(
  _input: SessionsCompactInput
): Promise<SessionsCompactResult> {
  throw new NotImplementedError(`exponential_sessions_compact`, `EXP-936`)
}
