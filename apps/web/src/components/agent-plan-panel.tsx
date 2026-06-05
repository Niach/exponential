import { useEffect, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  AlertTriangle,
  Check,
  HelpCircle,
  Loader2,
  Pencil,
  Send,
  Sparkles,
} from "lucide-react"
import type { Issue, IssueEvent } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { issueEventCollection } from "@/lib/collections"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"

// Agent lifecycle events shown in the quiet activity feed (and used here to
// detect a terminal error for the Retry affordance).
export const AGENT_EVENT_TYPES = new Set([
  `agent_started`,
  `plan_ready`,
  `agent_question`,
  `agent_answer`,
  `pr_opened`,
  `pr_merged`,
  `agent_error`,
])

type AgentStateData = {
  planText: string | null
  question: string | null
}

// First-class panel for the agent plan/question lifecycle, replacing the
// plan/question comment rows. State is driven by the synced `issue` columns;
// the plan/question TEXT is fetched via tRPC (server-only, not in Electric).
export function AgentPlanPanel({
  issue,
  canApprovePlan,
}: {
  issue: Issue
  canApprovePlan: boolean
}) {
  const state = issue.agentPlanState
  const [data, setData] = useState<AgentStateData | null>(null)
  const [busy, setBusy] = useState<
    null | `approve` | `request_changes` | `answer` | `retry`
  >(null)
  const [answer, setAnswer] = useState(``)

  const { data: events } = useLiveQuery(
    (query) =>
      query
        .from({ e: issueEventCollection })
        .where(({ e }) => eq(e.issueId, issue.id))
        .orderBy(({ e }) => e.createdAt),
    [issue.id]
  )

  const agentEvents = ((events ?? []) as IssueEvent[]).filter((e) =>
    AGENT_EVENT_TYPES.has(e.type)
  )
  const latestAgentEvent = agentEvents.at(-1)
  const latestIsError = latestAgentEvent?.type === `agent_error`

  // Fetch plan/question text whenever the relevant synced state/revision moves.
  useEffect(() => {
    if (
      state !== `awaiting_approval` &&
      state !== `awaiting_answer` &&
      state !== `approved`
    ) {
      setData(null)
      return
    }
    let cancelled = false
    void trpc.agentPlan.getState
      .query({ issueId: issue.id })
      .then((r) => {
        if (!cancelled) setData({ planText: r.planText, question: r.question })
      })
      .catch(() => {
        if (!cancelled) setData(null)
      })
    return () => {
      cancelled = true
    }
  }, [issue.id, state, issue.agentPlanRevision])

  // No agent involvement → render nothing.
  if (state == null && !latestIsError) return null

  const finished = issue.status === `done` || issue.status === `cancelled`
  const implementing =
    !finished &&
    state === `approved` &&
    !issue.prState &&
    !latestIsError

  const run = async (
    label: typeof busy,
    fn: () => Promise<unknown>,
    after?: () => void
  ) => {
    setBusy(label)
    try {
      await fn()
      after?.()
    } finally {
      setBusy(null)
    }
  }

  return (
    <div className="mx-4 my-3 rounded-md border border-border bg-card">
      <div className="flex items-center gap-2 border-b border-border px-3 py-2 text-xs">
        <Sparkles className="size-3.5 text-indigo-300" />
        <span className="font-medium text-foreground">Agent plan</span>
        {issue.agentPlanRevision > 0 && (
          <span className="text-muted-foreground">
            rev {issue.agentPlanRevision}
          </span>
        )}
        {state === `approved` && issue.agentPlanApprovedAt && (
          <span className="ml-auto inline-flex items-center gap-1 rounded-full bg-green-500/15 px-1.5 py-0.5 text-[10px] font-medium text-green-200">
            <Check className="size-2.5" />
            Approved
          </span>
        )}
      </div>

      <div className="px-3 py-2.5">
        {state === `drafting` && (
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Loader2 className="size-3 animate-spin text-indigo-300" />
            Agent is working on a plan…
          </div>
        )}

        {state === `awaiting_answer` && (
          <div className="space-y-2">
            <div className="flex items-center gap-2 text-xs text-amber-300">
              <HelpCircle className="size-3.5" />
              <span className="font-medium">The agent has a question</span>
            </div>
            <div className="text-sm text-foreground">
              {data?.question ? (
                <MarkdownEditor
                  markdown={data.question}
                  editable={false}
                  onChange={() => {}}
                />
              ) : (
                <span className="text-muted-foreground">Loading…</span>
              )}
            </div>
            {canApprovePlan && (
              <div className="flex items-end gap-2">
                <Textarea
                  value={answer}
                  onChange={(e) => setAnswer(e.target.value)}
                  placeholder="Answer the agent…"
                  className="min-h-16 text-sm"
                  disabled={busy !== null}
                />
                <Button
                  type="button"
                  size="icon"
                  aria-label="Send answer"
                  disabled={busy !== null || !answer.trim()}
                  onClick={() =>
                    void run(
                      `answer`,
                      () =>
                        trpc.agentPlan.answerQuestion.mutate({
                          issueId: issue.id,
                          answer: answer.trim(),
                        }),
                      () => setAnswer(``)
                    )
                  }
                >
                  {busy === `answer` ? (
                    <Loader2 className="size-4 animate-spin" />
                  ) : (
                    <Send className="size-4" />
                  )}
                </Button>
              </div>
            )}
          </div>
        )}

        {(state === `awaiting_approval` || state === `approved`) && (
          <div className="space-y-2">
            <div className="text-sm text-foreground">
              {data?.planText ? (
                <MarkdownEditor
                  markdown={data.planText}
                  editable={false}
                  onChange={() => {}}
                />
              ) : (
                <span className="text-muted-foreground">Loading plan…</span>
              )}
            </div>
            {state === `awaiting_approval` && canApprovePlan && (
              <div className="flex items-center gap-2">
                <Button
                  type="button"
                  size="xs"
                  disabled={busy !== null}
                  onClick={() =>
                    void run(`approve`, () =>
                      trpc.agentPlan.approvePlan.mutate({ issueId: issue.id })
                    )
                  }
                >
                  {busy === `approve` ? (
                    <Loader2 className="mr-1 size-3 animate-spin" />
                  ) : (
                    <Check className="mr-1 size-3" />
                  )}
                  Approve
                </Button>
                <Button
                  type="button"
                  size="xs"
                  variant="ghost"
                  disabled={busy !== null}
                  onClick={() =>
                    void run(`request_changes`, () =>
                      trpc.agentPlan.requestChanges.mutate({ issueId: issue.id })
                    )
                  }
                >
                  {busy === `request_changes` ? (
                    <Loader2 className="mr-1 size-3 animate-spin" />
                  ) : (
                    <Pencil className="mr-1 size-3" />
                  )}
                  Request changes
                </Button>
                <span className="text-xs text-muted-foreground">
                  or reply below to refine
                </span>
              </div>
            )}
          </div>
        )}

        {implementing && (
          <div className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
            <Loader2 className="size-3 animate-spin text-indigo-300" />
            Agent is implementing the approved plan…
          </div>
        )}

        {latestIsError && (
          <div className="flex items-center justify-between gap-2 rounded-md border border-destructive/40 bg-destructive/5 px-2.5 py-2 text-xs">
            <span className="flex items-center gap-1.5 text-muted-foreground">
              <AlertTriangle className="size-3.5 text-destructive" />
              The agent hit an error.
            </span>
            {canApprovePlan && (
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="h-6 px-2 text-xs"
                disabled={busy !== null}
                onClick={() =>
                  void run(`retry`, () =>
                    trpc.agentPlan.retry.mutate({ issueId: issue.id })
                  )
                }
              >
                {busy === `retry` ? `Retrying…` : `Retry`}
              </Button>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
