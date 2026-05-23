import type { CompanionConfig } from "./config"
import type { Logger } from "./logger"
import type { StateHandle, IssueRow } from "./state"
import { loadAccessToken } from "./github-auth"
import { getPullRequest, parsePrUrl } from "./github-api"
import { connectExponentialMcp } from "./exponential-mcp-client"
import { createWorktreeManager } from "./worktree"

const TICK_MS = 60_000
const ABANDON_AFTER_MS = 14 * 24 * 60 * 60_000

interface Args {
  config: CompanionConfig
  state: StateHandle
  log: Logger
}

async function reactToClosedPr(
  args: {
    issue: IssueRow
    merged: boolean
  } & Args
): Promise<void> {
  const { issue, merged, config, state, log } = args
  log.info(
    { issueId: issue.id, identifier: issue.identifier, merged },
    `PR closed; reconciling`
  )
  const wt = createWorktreeManager({ config, log })
  if (issue.worktreePath && issue.branch && issue.repoPath) {
    await wt
      .cleanup({
        worktreePath: issue.worktreePath,
        branch: issue.branch,
        repoPath: issue.repoPath,
        defaultBranch: `main`,
      })
      .catch((e: unknown) =>
        log.warn(
          { err: e instanceof Error ? e.message : String(e) },
          `worktree cleanup failed (best-effort)`
        )
      )
  }
  state.setIssueStatus(issue.id, merged ? `done` : `cancelled`)
  try {
    const mcp = await connectExponentialMcp(config)
    await mcp.updateIssueStatus({
      issueId: issue.id,
      status: merged ? `done` : `cancelled`,
    })
    await mcp.close()
  } catch (e) {
    log.warn(
      { err: e instanceof Error ? e.message : String(e) },
      `MCP status update failed for closed PR`
    )
  }
}

export interface PrPollLoop {
  stop(): void
}

export function startPrPollLoop(args: Args): PrPollLoop {
  const { config, state, log } = args
  let stopped = false

  const tick = async () => {
    if (stopped) return
    const auth = await loadAccessToken().catch(() => null)
    if (!auth) return

    const pending = state.listIssues({ status: [`in_review`, `pushed`] })
    for (const issue of pending) {
      if (!issue.prUrl) continue
      const parsed = parsePrUrl(issue.prUrl)
      if (!parsed) {
        log.warn(
          { issueId: issue.id, prUrl: issue.prUrl },
          `could not parse PR URL`
        )
        continue
      }
      try {
        const pr = await getPullRequest(auth.token, parsed)
        if (pr.merged) {
          await reactToClosedPr({ issue, merged: true, config, state, log })
          continue
        }
        if (pr.state === `closed`) {
          const closedAt = pr.closedAt ? Date.parse(pr.closedAt) : Date.now()
          if (Date.now() - closedAt > ABANDON_AFTER_MS) {
            await reactToClosedPr({
              issue,
              merged: false,
              config,
              state,
              log,
            })
          }
        }
      } catch (e) {
        log.warn(
          {
            issueId: issue.id,
            err: e instanceof Error ? e.message : String(e),
          },
          `PR poll failed`
        )
      }
    }
  }

  void tick()
  const timer = setInterval(() => void tick(), TICK_MS)
  return {
    stop: () => {
      stopped = true
      clearInterval(timer)
    },
  }
}
