import type { CompanionConfig } from "./config"
import type { IssueRow, StateHandle } from "./state"
import type { Logger } from "./logger"

export interface IssueEvent {
  type: `assigned` | `unassigned` | `updated`
  issueId: string
  identifier: string
  title: string
  projectId: string
  assigneeId: string | null
}

export interface Dispatcher {
  enqueue(event: IssueEvent): void
  stop(): Promise<void>
}

export interface IssuePipelineDeps {
  config: CompanionConfig
  state: StateHandle
  log: Logger
}

/**
 * Implementation of the per-issue pipeline (worktree → driver → tests → PR).
 * Wired in by task B4. For now this is a placeholder that walks the state
 * machine to `needs_human` so we can exercise the dispatcher in isolation.
 */
export type IssuePipeline = (
  issue: IssueRow,
  deps: IssuePipelineDeps
) => Promise<void>

interface Args {
  config: CompanionConfig
  state: StateHandle
  log: Logger
  pipeline?: IssuePipeline
}

const PLACEHOLDER_PIPELINE: IssuePipeline = async (issue, { state, log }) => {
  log.warn(
    { issueId: issue.id, identifier: issue.identifier },
    `pipeline not yet wired (task B4) — marking needs_human`
  )
  state.setIssueStatus(
    issue.id,
    `needs_human`,
    `pipeline not implemented (task B4)`
  )
}

const NON_TERMINAL_STATUSES = [
  `queued`,
  `claimed`,
  `planning`,
  `awaiting_approval`,
  `coding`,
  `pushed`,
] as const

// Statuses the dispatcher can re-enter the pipeline from when it sees an
// `updated` event. `awaiting_approval` is here because the human side of the
// plan flow (approving, requesting changes, leaving comments) happens via
// row updates we need to react to. `needs_human` is here so the web Retry
// button actually re-runs the pipeline after the user has fixed whatever
// the daemon needed help with (e.g. linking a GitHub repo). The pipeline
// itself dedupes its needs_human comment posts so we don't loop when the
// underlying problem hasn't been resolved yet.
const REENTRY_STATUSES = [
  `queued`,
  `cancelled`,
  `failed`,
  `awaiting_approval`,
  `needs_human`,
] as const

export function startDispatcher(args: Args): Dispatcher {
  const { config, state, log } = args
  const pipeline = args.pipeline ?? PLACEHOLDER_PIPELINE
  const maxConcurrent = config.driver.maxConcurrentIssues
  const queue: string[] = []
  const running = new Set<string>()
  let stopped = false

  // Boot-time recovery: scan for in-flight issues (status != terminal) and
  // re-enqueue them. Reset 'coding' back to 'claimed' so the pipeline picks
  // up cleanly.
  function recoverInFlight() {
    const stuck = state.listIssues({ status: [...NON_TERMINAL_STATUSES] })
    for (const issue of stuck) {
      // Reset transient executing states. awaiting_approval is left alone —
      // the human gate hasn't moved just because we restarted.
      if (issue.status === `coding` || issue.status === `planning`) {
        state.setIssueStatus(issue.id, `claimed`, `resumed after restart`)
      }
      enqueueId(issue.id)
    }
    if (stuck.length > 0) {
      log.info({ count: stuck.length }, `recovered in-flight issues`)
    }
  }

  function enqueueId(issueId: string) {
    if (queue.includes(issueId) || running.has(issueId)) return
    queue.push(issueId)
    setImmediate(drain)
  }

  function drain() {
    if (stopped) return
    while (running.size < maxConcurrent && queue.length > 0) {
      const id = queue.shift()!
      const issue = state.getIssue(id)
      if (!issue) {
        log.warn({ issueId: id }, `drain: issue vanished from state`)
        continue
      }
      running.add(id)
      const start = Date.now()
      log.info(
        { issueId: id, identifier: issue.identifier, status: issue.status },
        `pipeline start`
      )
      pipeline(issue, { config, state, log })
        .catch((err: unknown) => {
          const message =
            err instanceof Error ? err.message : String(err)
          log.error(
            { issueId: id, err: message },
            `pipeline threw`
          )
          state.setIssueStatus(id, `failed`, message)
        })
        .finally(() => {
          running.delete(id)
          log.info(
            { issueId: id, durationMs: Date.now() - start },
            `pipeline end`
          )
          if (!stopped) setImmediate(drain)
        })
    }
  }

  recoverInFlight()

  return {
    enqueue(event) {
      const isOwn = event.assigneeId !== null
      if (event.type === `unassigned` || !isOwn) {
        // No longer ours. Mark any in-flight work cancelled.
        const issue = state.getIssue(event.issueId)
        if (
          issue &&
          (NON_TERMINAL_STATUSES as readonly string[]).includes(issue.status)
        ) {
          log.info(
            { issueId: event.issueId },
            `issue unassigned from bot; cancelling`
          )
          state.setIssueStatus(event.issueId, `cancelled`, `unassigned`)
        }
        return
      }

      const existing = state.getIssue(event.issueId)

      // `assigned` is an explicit user signal — always honor it. If the row
      // was sitting in any state at all (needs_human after a previous failure,
      // done after a closed PR, …), reset and re-queue. The next pipeline tick
      // will call `agentPlan.resetPlan` to clear any stale server-side plan.
      if (event.type === `assigned`) {
        state.upsertIssue({
          id: event.issueId,
          identifier: event.identifier,
          title: event.title,
          projectId: event.projectId,
          status: `queued`,
        })
        state.patchIssue(event.issueId, {
          lastError: null,
          planRevision: 0,
          prUrl: null,
          worktreePath: null,
          branch: null,
        })
        enqueueId(event.issueId)
        return
      }

      // `updated` events fire frequently (any row change, including comment
      // INSERTs via our trigger). Only re-enter from a small allowlist.
      if (
        existing &&
        !(REENTRY_STATUSES as readonly string[]).includes(existing.status)
      ) {
        return
      }

      state.upsertIssue({
        id: event.issueId,
        identifier: event.identifier,
        title: event.title,
        projectId: event.projectId,
        status: existing?.status === `awaiting_approval`
          ? `awaiting_approval`
          : `queued`,
      })
      enqueueId(event.issueId)
    },
    async stop() {
      stopped = true
      // Wait briefly for in-flight pipelines to settle. We don't kill them —
      // they have their own AbortSignal hooked up to process signals.
      const start = Date.now()
      while (running.size > 0 && Date.now() - start < 5_000) {
        await new Promise((r) => setTimeout(r, 100))
      }
      if (running.size > 0) {
        log.warn(
          { running: Array.from(running) },
          `dispatcher.stop: pipelines still running after 5s grace`
        )
      }
    },
  }
}
