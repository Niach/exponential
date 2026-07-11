import { and, eq, isNotNull } from "drizzle-orm"
import { db } from "@/db/connection"
import { issues, projects, releases } from "@/db/schema"
import { fetchPullState, resolveRepoToken } from "@/lib/integrations/github-pr"
import {
  applyPrClosedState,
  applyPrMergeState,
  applyReleasePrClosedState,
  applyReleasePrMergeState,
} from "@/lib/integrations/pr-sync"

const POLL_INTERVAL_MS = 3 * 60 * 1000 // every 3 minutes
const INITIAL_DELAY_MS = 30 * 1000 // first run ~30s after boot

let running = false

// Parse `https://github.com/<owner>/<repo>/pull/<n>` → "owner/repo". Returns
// null for anything that doesn't match (we skip those rows).
function parseRepoFromPrUrl(prUrl: string): string | null {
  const match = prUrl.match(
    /github\.com\/([^/]+)\/([^/]+)\/pull\/\d+/i
  )
  if (!match) return null
  return `${match[1]}/${match[2]}`
}

// One poll pass: outbound-check every open-PR issue's GitHub state and flip it
// to merged when it's been merged. This is the SELF-HOSTED merge-detection
// trigger — works without inbound reachability (no webhook required).
async function pollOpenPrs(): Promise<void> {
  if (running) return
  running = true
  try {
    const rows = await db
      .select({
        issueId: issues.id,
        prUrl: issues.prUrl,
        prNumber: issues.prNumber,
        workspaceId: projects.workspaceId,
      })
      .from(issues)
      .innerJoin(projects, eq(projects.id, issues.projectId))
      .where(
        and(
          eq(issues.prState, `open`),
          isNotNull(issues.prNumber),
          isNotNull(issues.prUrl)
        )
      )

    for (const row of rows) {
      if (!row.prUrl || row.prNumber == null) continue
      try {
        const repo = parseRepoFromPrUrl(row.prUrl)
        if (!repo) continue
        const token = await resolveRepoToken({
          workspaceId: row.workspaceId,
          repo,
        })
        const state = await fetchPullState(repo, row.prNumber, token)
        if (state.merged) {
          await applyPrMergeState({
            issueId: row.issueId,
            prUrl: row.prUrl,
            mergedAt: new Date(),
            actorUserId: null,
          })
        } else if (state.state === `closed`) {
          // Closed without merging — flip to closed, which also drops the
          // row out of this poller's prState='open' re-fetch set.
          await applyPrClosedState({
            issueId: row.issueId,
            prUrl: row.prUrl,
          })
        }
      } catch (err) {
        // One repo failing must not abort the batch.
        console.error(`[pr-merge-poll] issue ${row.issueId}:`, err)
      }
    }

    // Release PRs (EXP-56): same outbound check for open release PRs; merge
    // auto-ships the release (applyReleasePrMergeState stamps shippedAt).
    const releaseRows = await db
      .select({
        releaseId: releases.id,
        prUrl: releases.prUrl,
        prNumber: releases.prNumber,
        workspaceId: releases.workspaceId,
      })
      .from(releases)
      .where(
        and(
          eq(releases.prState, `open`),
          isNotNull(releases.prNumber),
          isNotNull(releases.prUrl)
        )
      )

    for (const row of releaseRows) {
      if (!row.prUrl || row.prNumber == null) continue
      try {
        const repo = parseRepoFromPrUrl(row.prUrl)
        if (!repo) continue
        const token = await resolveRepoToken({
          workspaceId: row.workspaceId,
          repo,
        })
        const state = await fetchPullState(repo, row.prNumber, token)
        if (state.merged) {
          await applyReleasePrMergeState({
            releaseId: row.releaseId,
            prUrl: row.prUrl,
            mergedAt: new Date(),
          })
        } else if (state.state === `closed`) {
          await applyReleasePrClosedState({
            releaseId: row.releaseId,
            prUrl: row.prUrl,
          })
        }
      } catch (err) {
        console.error(`[pr-merge-poll] release ${row.releaseId}:`, err)
      }
    }
  } catch (err) {
    console.error(`[pr-merge-poll] poll failed:`, err)
  } finally {
    running = false
  }
}

// Start the outbound PR-merge poller, gated on GITHUB_POLLING (decoupled from
// SELF_HOSTED — reachability ≠ deployment type). Set it on instances GitHub
// can't reach by webhook (LAN / NAT); reachable instances use the App webhook
// instead and leave it unset. Never enable on cloud (per-user polling doesn't
// scale). Fire-and-forget; safe to call once at boot.
export function bootstrapSelfHosted(): void {
  if (process.env.GITHUB_POLLING !== `true`) return
  setTimeout(() => {
    void pollOpenPrs()
  }, INITIAL_DELAY_MS)
  setInterval(() => {
    void pollOpenPrs()
  }, POLL_INTERVAL_MS)
}
