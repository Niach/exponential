import { and, eq, gte, inArray, or } from "drizzle-orm"
import { db } from "@/db/connection"
import { pullRequests } from "@/db/schema"
import { forgeFor, type ForgePullRequest, type ForgeRepo } from "@/lib/ecode/forge"
import {
  githubBudgetLowForToken,
  isGithubRateLimited,
} from "@/lib/integrations/github-client"
import { repairPullRequestChecksIfStale } from "@/lib/integrations/pr-checks"
import { resolveAppUserForGithubActor } from "@/lib/integrations/github-identity"
import {
  applyPullRequestBase,
  applyPullRequestClosed,
  applyPullRequestMerged,
  applyPullRequestReopened,
} from "@/lib/integrations/pr-sync"
import { LIVE_PR_STATES } from "@/lib/pr-merge-guard"

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
  // `draft` is a live PR like `open`.
  localPrState: string | null,
  // Only the transition matters here — deliberately narrower than PullState so
  // the decision can't quietly start depending on attribution fields.
  state: Pick<ForgePullRequest, `state` | `merged`>
): PrPollAction {
  if (state.merged) return localPrState === `merged` ? `none` : `merge`
  if (state.state === `closed`) {
    return localPrState === `open` || localPrState === `draft` ? `close` : `none`
  }
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
    // EXP-1291: every tracked PR is a `pull_requests` row (an issue's, a
    // chat's, an adopted one): live ones, plus recently closed ones (a
    // reopen, and the merge that may follow it, must be seen at all).
    const rows = await db
      .select({
        id: pullRequests.id,
        url: pullRequests.url,
        number: pullRequests.number,
        state: pullRequests.state,
        base: pullRequests.base,
        teamId: pullRequests.teamId,
      })
      .from(pullRequests)
      .where(
        or(
          inArray(pullRequests.state, [...LIVE_PR_STATES]),
          and(eq(pullRequests.state, `closed`), gte(pullRequests.updatedAt, closedCutoff))
        )
      )

    let backedOff = 0
    for (const row of rows) {
      try {
        const repo = parseRepoFromPrUrl(row.url)
        if (!repo) continue
        // #EXP-1614: the repo's forge acts with its own credential.
        const bare: ForgeRepo = { id: ``, fullName: repo }
        const token = (await (await forgeFor(bare)).credentials(bare))?.token ?? null
        // EXP-1609: a low installation budget skips this PR until the reset
        // — interactive calls (an agent's `pr` op, a person's merge) keep
        // the rest; the next pass after the reset catches up.
        if (githubBudgetLowForToken(token)) {
          backedOff++
          continue
        }
        if (await reconcilePullRequestState(row, repo, token)) backedOff++
      } catch (err) {
        if (isGithubRateLimited(err)) {
          backedOff++
          continue
        }
        // One repo failing must not abort the batch.
        console.error(`[pr-merge-poll] ${row.url}:`, err)
      }
    }
    if (backedOff > 0) {
      console.warn(`[pr-merge-poll] GitHub budget low: ${backedOff} PR(s) left for the next pass`)
    }
  } catch (err) {
    console.error(`[pr-merge-poll] poll failed:`, err)
  } finally {
    running = false
  }
}

export interface PolledPullRequest {
  id: string
  number: number
  state: string
  base: string
}

/**
 * Re-read one tracked PR's GitHub state and apply what it implies (merge,
 * close, reopen, a moved base; a live PR's unsettled checks). Shared by the
 * self-hosted poller and the watchdog's live-PR reconciler (EXP-1706).
 * → true when a low budget left the checks repair for later.
 */
export async function reconcilePullRequestState(
  row: PolledPullRequest,
  repo: string,
  token: string | null
): Promise<boolean> {
  const at: ForgeRepo = { id: ``, fullName: repo, token }
  const state = await (await forgeFor(at)).readPullRequest(at, row.number)
  if (!state) throw new Error(`${repo}#${row.number} is gone from its forge`)
  // A live PR's base moves on GitHub (a retarget on github.com, or
  // GitHub's restack when the PR below lands); a polling instance
  // never gets the `edited` webhook, so the pass re-reads it. A merge
  // carries the base it saw into applyPullRequestMerged instead, so
  // the landed check runs on the base GitHub merged it into.
  if (state.state === `open` && !state.merged && state.baseRef && state.baseRef !== row.base) {
    await applyPullRequestBase({ pullRequestId: row.id, base: state.baseRef })
  }
  switch (decidePrPollAction(row.state, state)) {
    case `merge`:
      await applyPullRequestMerged({
        pullRequestId: row.id,
        mergedAt: new Date(),
        base: state.baseRef,
        actorUserId: null,
        // EXP-617: `merged_by` off the poll response is a self-hosted
        // instance's only attribution.
        githubActorUserId: await resolveAppUserForGithubActor(state.mergedBy),
      })
      break
    case `close`:
      await applyPullRequestClosed({ pullRequestId: row.id })
      break
    case `reopen`:
      await applyPullRequestReopened({ pullRequestId: row.id, base: state.baseRef })
      break
    case `none`:
      break
  }
  // EXP-1608: no webhooks reach a polling instance, so no check_run
  // either: a still-open PR whose checks are unsettled (or were read at
  // an older head) is repaired here, inside the same budget back-off.
  // A missing checks:read is negative-cached per installation in
  // pr-checks, so it costs nothing after the first refusal.
  if (state.state === `open` && !state.merged && (LIVE_PR_STATES as readonly string[]).includes(row.state)) {
    if (githubBudgetLowForToken(token)) return true
    await repairPullRequestChecksIfStale(row.id, state.headSha ?? null)
  }
  return false
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
