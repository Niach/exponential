// EXP-936: `exponential_sessions_compact` — the run asks its HOST to compact
// the agent's context.
//
// Compaction is a REQUEST, never an act: the server relays a `compact_request`
// frame (steer protocol, `ServerFrame`) to the run's publisher and the HOST
// executes it at the next turn boundary (the device holds the compaction
// until the running turn ends, sends the agent's own `/compact <keep>`, and
// prompts it to continue once the summary is in — an unattended run must not
// idle after it asked). The verdict comes back on the same call: the relay
// awaits the publisher's `compact_verdict` and answers the HTTP request with
// it, so the agent reads it off its tool result and no viewer ever hears of a
// refusal.
//
// Who decides what:
//
//  - this file: the CALLING session only (the `X-Exp-Session-Id` run, its
//    owner or its host) — anything else is `not_own_session`; a run whose
//    agent is not one of the contract's (`codingAgent`) is
//    `unsupported_agent` (both contract agents have a `/compact`);
//  - the device: `too_early` below 50 % context use, `cooldown` within 20
//    turns of the last compaction or while one is already open or pending —
//    it holds the context meter and the turn count, the server only carries
//    the verdict back;
//  - `keep` is carried verbatim into the compaction prompt (what the summary
//    must preserve); `reason` is the tool card's subject on the run's feed
//    (contract `expToolDisplay` `sessions_compact`) and logged here.
//
// A run with no live publisher, a host that never answers (older than the
// frame) or a relay that is off are ERRORS, not refusals: the agent must not
// read "refused" into "nobody was there to ask".
import { eq } from "drizzle-orm"
import { contract } from "@exp/domain-contract"
import { codingSessions } from "@/db/schema"
import { db } from "@/db/connection"
import {
  getSteerRelayConfig,
  relayPostCompact,
  type RelayCompactOutcome,
} from "@/lib/steer"

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

/** The row facts the ownership + agent checks need. */
export interface SessionsCompactRow {
  id: string
  userId: string
  hostUserId: string | null
  status: string
  agent: string | null
}

/** The two seams a test replaces: the row lookup and the relay hop. `relay`
 * resolves `null` when remote steering is not configured on this server. */
export interface SessionsCompactDeps {
  loadSession: (sessionId: string) => Promise<SessionsCompactRow | null>
  relay: (
    sessionId: string,
    keep: string | undefined
  ) => Promise<RelayCompactOutcome | null>
  log: (line: string) => void
}

const defaultDeps: SessionsCompactDeps = {
  loadSession: async (sessionId) => {
    const [row] = await db
      .select({
        id: codingSessions.id,
        userId: codingSessions.userId,
        hostUserId: codingSessions.hostUserId,
        status: codingSessions.status,
        agent: codingSessions.agent,
      })
      .from(codingSessions)
      .where(eq(codingSessions.id, sessionId))
      .limit(1)
    return row ?? null
  },
  relay: async (sessionId, keep) => {
    const config = getSteerRelayConfig()
    if (!config) return null
    return relayPostCompact(config, sessionId, keep)
  },
  log: (line) => console.info(line),
}

/** Why a delivery failure is an error the agent can act on, by reason. */
export function describeCompactDeliveryFailure(
  reason: Extract<RelayCompactOutcome, { delivered: false }>[`reason`]
): string {
  switch (reason) {
    case `no_publisher`:
      return `The run's host is not connected to the steer relay right now; compaction needs a live host. Try again in a moment.`
    case `no_verdict`:
      return `The run's host did not answer the compaction request; it may predate this feature. Update the desktop app or the exponential CLI on that machine.`
    case `busy`:
      return `A compaction request for this run is already waiting for the host's answer.`
    case `relay_error`:
      return `The steer relay did not answer the compaction request.`
  }
}

export async function requestSessionCompaction(
  input: SessionsCompactInput,
  deps: SessionsCompactDeps = defaultDeps
): Promise<SessionsCompactResult> {
  const row = await deps.loadSession(input.sessionId)
  if (!row) throw new Error(`Session not found`)
  if (row.userId !== input.userId && row.hostUserId !== input.userId) {
    return { accepted: false, refusedBecause: `not_own_session` }
  }
  if (row.status === `ended`) {
    throw new Error(`This run has ended; there is no context left to compact.`)
  }
  const agents = contract.codingAgent.values as readonly string[]
  if (!row.agent || !agents.includes(row.agent)) {
    return { accepted: false, refusedBecause: `unsupported_agent` }
  }
  deps.log(
    `[mcp] session ${input.sessionId} asked to compact its context: ${input.reason}`
  )
  const outcome = await deps.relay(input.sessionId, input.keep)
  if (!outcome) {
    throw new Error(
      `Remote steering is not configured on this server (STEER_RELAY_URL), so a compaction request cannot reach the run's host.`
    )
  }
  if (!outcome.delivered) {
    throw new Error(describeCompactDeliveryFailure(outcome.reason))
  }
  if (outcome.accepted) return { accepted: true }
  return { accepted: false, refusedBecause: outcome.refusedBecause ?? `cooldown` }
}
