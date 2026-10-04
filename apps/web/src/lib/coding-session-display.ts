import type { CodingSession } from "@/db/schema"
import { relativeTime } from "@/components/comment-rows/format"

/** How a LIVE run renders (EXP-1184; EXP-214/531/848 before it) — one rule
 * ×4, locked by `packages/domain-contract/fixtures/session-display.json`
 * (desktop `queries::coding_session_display`, iOS CodingSessionDisplay.swift,
 * Android CodingSessionDisplay.kt). First match wins:
 * - `needs_input`: the run waits on a person (a pending ask, or an MCP
 *   question to its user) — on every live status, an open PR included: the
 *   server clears the flag on every turn start and every PR park, so it is
 *   never stale.
 * - `working`: the agent is mid-turn (the device-written `agent_busy`) — a
 *   follow-up turn on an `in_review` run included.
 * - `review`: idle with its PR open (`in_review`, the PR neither merged nor
 *   closed).
 * - `done`: idle with no open PR, or merged.
 * The session row's status never changes for this. A paused (offline) run
 * and an ended row are the caller's to decide. */
export type SessionDisplayState = `needs_input` | `working` | `review` | `done`

export function sessionDisplayState(
  session: Pick<CodingSession, `status` | `needsInput` | `agentBusy`>,
  prState: string | null | undefined
): SessionDisplayState {
  if (session.needsInput) return `needs_input`
  if (session.agentBusy) return `working`
  const prOpen = prState !== `merged` && prState !== `closed`
  return session.status === `in_review` && prOpen ? `review` : `done`
}

/** EXP-848: whether a row animates — the agent is executing a turn right now
 * on a row that is still live (an ended row never does, whatever the flag
 * says). Same fixture as `sessionDisplayState`. */
export function sessionRowIsWorking(
  session: Pick<CodingSession, `status` | `needsInput` | `agentBusy`>,
  prState: string | null | undefined
): boolean {
  return (
    session.status !== `ended` &&
    sessionDisplayState(session, prState) === `working`
  )
}

/** EXP-850 §8: the second line of a session list row — the device-written
 * `agent_caption` (today the caption of the run's newest running workflow,
 * `workflowCaption`), or null. Every list renders it BEFORE the device
 * byline, and only while the run is live: the server clears the column on
 * every path that ends a row, and an ended row that a stale client still
 * holds must not keep narrating. Hand-mirrored with `sessionDisplayState` ×4
 * (desktop `queries::session_agent_caption`, which additionally prefers the
 * in-process caption signal for a run its own engine hosts). */
export function sessionAgentCaption(
  session: Pick<CodingSession, `status` | `agentCaption`>
): string | null {
  if (session.status === `ended` || session.status === `merged`) return null
  const caption = session.agentCaption?.trim()
  return caption ? caption : null
}

/** The tone a session row's status line paints in. */
export type SessionStatusTone = `muted` | `amber` | `emerald` | `sky`

/** EXP-874: the status line of a LIVE session list row (Android's row is
 * the reference) — the parked state first, then the host machine; a live run
 * reads "<device> · started <rel time>". `device` is the resolved label
 * (`device.label || session.deviceLabel || 'Desktop'`). A paused run (offline
 * machine) beats every state: the agent is parked, not gone. */
export function sessionStatusLine({
  state,
  paused,
  device,
  startedAt,
}: {
  state: SessionDisplayState
  paused: boolean
  device: string
  startedAt: Date | string | null | undefined
}): { text: string; tone: SessionStatusTone } {
  if (paused) return { text: `Paused · ${device}`, tone: `muted` }
  switch (state) {
    case `needs_input`:
      return { text: `Needs input · ${device}`, tone: `amber` }
    case `review`:
      return { text: `Ready for review · ${device}`, tone: `emerald` }
    case `done`:
      return { text: `Done · ${device}`, tone: `sky` }
    case `working`: {
      const started = relativeTime(startedAt)
      return {
        text: started ? `${device} · started ${started}` : device,
        tone: `muted`,
      }
    }
  }
}

/** EXP-1184: the Work face strip's Run-tab mark for a live run — whose mark,
 * and what it is doing (no run = no mark). */
export function runFaceMark(
  session:
    | (Pick<CodingSession, `status` | `needsInput` | `agentBusy`> & {
        agent: string | null
      })
    | null
    | undefined,
  prState: string | null | undefined
): { agent: string | null | undefined; state: SessionDisplayState | undefined } {
  if (!session || session.status === `ended`) {
    return { agent: session?.agent, state: undefined }
  }
  return { agent: session.agent, state: sessionDisplayState(session, prState) }
}
