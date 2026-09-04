import { and, eq, gte, isNotNull, isNull, or } from "drizzle-orm"
import { db } from "@/db/connection"
import { issues, boards, codingSessions } from "@/db/schema"
import type { PullState } from "@/lib/integrations/github-pr"
import { fetchPullState, resolveRepoToken } from "@/lib/integrations/github-pr"
import { resolveAppUserForGithubActor } from "@/lib/integrations/github-identity"
import {
  applyPrClosedState,
  applyPrMergeState,
  applyPrReopenedState,
  applySessionPrState,
} from "@/lib/integrations/pr-sync"

const POLL_INTERVAL_MS = 3 * 60 * 1000 // every 3 minutes
const INITIAL_DELAY_MS = 30 * 1000 // first run ~30s after boot

// How long a closed-without-merge PR keeps being re-checked (REV2-74). A
// polling instance never receives the `reopened` webhook, so the only way it
// can learn about a close→reopen→merge sequence is to keep asking — but
// re-asking about every PR ever closed would grow each pass without bound.
export const CLOSED_PR_RECHECK_WINDOW_MS = 14 * 24 * 60 * 60 * 1000

let running = false

// Parse `https://github.com/<owner>/<repo>/pull/<n>` → "owner/repo". Returns
// null for anything that doesn't match (we skip those rows).
export function parseRepoFromPrUrl(prUrl: string): string | null {
  const match = prUrl.match(
    /github\.com\/([^/]+)\/([^/]+)\/pull\/\d+/i
  )
  if (!match) return null
  return `${match[1]}/${match[2]}`
}

export type PrPollAction = `merge` | `close` | `reopen` | `none`

// Pure dispatch: what the state GitHub just reported means for a locally
// `open`/`closed` PR. Both heals matter here — the webhook's close/reopen
// events never reach a polling instance, and a reopened PR that stays
// `closed` locally would never have its merge observed either (REV2-74).
export function decidePrPollAction(
  localPrState: string | null,
  // Only the transition matters here — deliberately narrower than PullState so
  // the decision can't quietly start depending on attribution fields.
  state: Pick<PullState, `state` | `merged`>
): PrPollAction {
  if (state.merged) return localPrState === `merged` ? `none` : `merge`
  if (state.state === `closed`) return localPrState === `open` ? `close` : `none`
  return localPrState === `closed` ? `reopen` : `none`
}

// One poll pass: outbound-check every tracked PR's GitHub state and apply the
// merge/close/reopen transition it implies. This is the SELF-HOSTED
// merge-detection trigger — works without inbound reachability (no webhook
// required). Exported for tests; the scheduler below is the only caller.
export async function runPrPollPass(now: Date = new Date()): Promise<void> {
  if (running) return
  running = true
  try {
    const closedCutoff = new Date(now.getTime() - CLOSED_PR_RECHECK_WINDOW_MS)
    const rows = await db
      .select({
        issueId: issues.id,
        prUrl: issues.prUrl,
        prNumber: issues.prNumber,
        prState: issues.prState,
        teamId: boards.teamId,
      })
      .from(issues)
      .innerJoin(boards, eq(boards.id, issues.boardId))
      .where(
        and(
          isNotNull(issues.prNumber),
          isNotNull(issues.prUrl),
          or(
            eq(issues.prState, `open`),
            // Recently closed and never merged: still worth asking about, so a
            // reopen (and the merge that may follow it) is seen at all.
            and(
              eq(issues.prState, `closed`),
              isNull(issues.prMergedAt),
              gte(issues.updatedAt, closedCutoff)
            )
          )
        )
      )

    // A batch PR is linked to several issues (same prUrl on every row) —
    // fetch each PR's state once per pass.
    const pullStates = new Map<string, PullState>()

    for (const row of rows) {
      if (!row.prUrl || row.prNumber == null) continue
      try {
        const repo = parseRepoFromPrUrl(row.prUrl)
        if (!repo) continue
        let state = pullStates.get(row.prUrl)
        if (!state) {
          const token = await resolveRepoToken({
            teamId: row.teamId,
            repo,
          })
          state = await fetchPullState(repo, row.prNumber, token)
          pullStates.set(row.prUrl, state)
        }
        switch (decidePrPollAction(row.prState, state)) {
          case `merge`:
            await applyPrMergeState({
              issueId: row.issueId,
              prUrl: row.prUrl,
              mergedAt: new Date(),
              actorUserId: null,
              // EXP-617: self-hosted has no webhook, so `merged_by` off the
              // poll response is its only chance to attribute the merge — and
              // to keep the person who pressed Merge out of their own
              // notification.
              githubActorUserId: await resolveAppUserForGithubActor(
                state.mergedBy
              ),
            })
            break
          case `close`:
            await applyPrClosedState({
              issueId: row.issueId,
              prUrl: row.prUrl,
            })
            break
          case `reopen`:
            await applyPrReopenedState({
              issueId: row.issueId,
              prUrl: row.prUrl,
            })
            break
          case `none`:
            break
        }
      } catch (err) {
        // One repo failing must not abort the batch.
        console.error(`[pr-merge-poll] issue ${row.issueId}:`, err)
      }
    }

    // EXP-734: the chore PRs of action/chat runs live on their session rows,
    // not on any issue — poll them the same way (same transitions, same
    // recheck window) so a self-hosted run's own PR merges and ends it.
    const sessionRows = await db
      .select({
        sessionId: codingSessions.id,
        prUrl: codingSessions.prUrl,
        prNumber: codingSessions.prNumber,
        prState: codingSessions.prState,
        teamId: codingSessions.teamId,
      })
      .from(codingSessions)
      .where(
        and(
          isNull(codingSessions.issueId),
          isNotNull(codingSessions.prNumber),
          isNotNull(codingSessions.prUrl),
          or(
            eq(codingSessions.prState, `open`),
            and(
              eq(codingSessions.prState, `closed`),
              gte(codingSessions.updatedAt, closedCutoff)
            )
          )
        )
      )
    for (const row of sessionRows) {
      if (!row.prUrl || row.prNumber == null) continue
      try {
        const repo = parseRepoFromPrUrl(row.prUrl)
        if (!repo) continue
        let state = pullStates.get(row.prUrl)
        if (!state) {
          const token = await resolveRepoToken({ teamId: row.teamId, repo })
          state = await fetchPullState(repo, row.prNumber, token)
          pullStates.set(row.prUrl, state)
        }
        switch (decidePrPollAction(row.prState, state)) {
          case `merge`:
            await applySessionPrState({ prUrl: row.prUrl, state: `merged` })
            break
          case `close`:
            await applySessionPrState({ prUrl: row.prUrl, state: `closed` })
            break
          case `reopen`:
            await applySessionPrState({ prUrl: row.prUrl, state: `open` })
            break
          case `none`:
            break
        }
      } catch (err) {
        console.error(`[pr-merge-poll] session ${row.sessionId}:`, err)
      }
    }
  } catch (err) {
    console.error(`[pr-merge-poll] poll failed:`, err)
  } finally {
    running = false
  }
}

// Start the outbound PR-merge poller, gated on GITHUB_POLLING (decoupled from
// CLOUD_INSTANCE — reachability ≠ deployment type). Set it on instances GitHub
// can't reach by webhook (LAN / NAT); reachable instances use the App webhook
// instead and leave it unset. Never enable on cloud (per-user polling doesn't
// scale). Fire-and-forget; safe to call once at boot.
export function bootstrapSelfHosted(): void {
  if (process.env.GITHUB_POLLING !== `true`) return
  setTimeout(() => {
    void runPrPollPass()
  }, INITIAL_DELAY_MS)
  setInterval(() => {
    void runPrPollPass()
  }, POLL_INTERVAL_MS)
}
