import type { CodingSession } from "@/db/schema"

/** How a live session should render (EXP-214) — the session status alone is
 * not the whole story: `in_review` splits on the linked issue's PR outcome
 * (merged → the run is done, green review otherwise, matching the issue-status
 * palette — a lagging self-host server can still leave a merged PR's row in
 * `in_review`), and a desktop-reported pending picker (plan approval /
 * AskUserQuestion) shows as "needs input" while the run is still coding.
 * EXP-531: `in_review` beats `needsInput` — once the PR is open the session
 * IS in review, and a stale flag (the desktop's post-turn idle nudge, or a
 * row written by an older server that never cleared it) must not mask "Ready
 * for review". Hand-mirrored on iOS (CodingSessionDisplay.swift), Android
 * (CodingSessionDisplay.kt) and desktop (queries.rs) — move all four in
 * lockstep. */
export type SessionDisplayState = `needs_input` | `running` | `review` | `done`

export function sessionDisplayState(
  session: Pick<CodingSession, `status` | `needsInput`>,
  prState: string | null | undefined
): SessionDisplayState {
  const merged = prState === `merged`
  if (session.status === `in_review`) return merged ? `done` : `review`
  if (session.needsInput && !merged) return `needs_input`
  return `running`
}
