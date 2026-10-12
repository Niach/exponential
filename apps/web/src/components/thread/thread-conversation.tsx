import { useState } from "react"
import {
  AgentRunMark,
  ExponentialLogo,
  ThreadFoldedRun,
  ThreadLinkRow,
  ThreadParallelCard,
  ThreadRunHeader,
  type RunMarkState,
} from "@exp/ui"
import { IssueChip } from "@/components/issue-chip"
import { ThreadRunBody } from "@/components/thread/thread-run-body"
import { sessionIdentity } from "@/lib/session-identity"
import { compactRelativeTime } from "@/lib/relative-time"
import type {
  ThreadBlock,
  ThreadLinkRow as ThreadLinkRowModel,
  ThreadRunState,
} from "@/lib/thread-view"
import type { ThreadBundle } from "@/hooks/use-threads"

// EXP-1284: the thread's CONVERSATION child — the model's blocks in order
// (`buildThreadView().conversation`):
//   * a `run` block: its header (agent mark, `Run N`, device · resumed ·
//     started by Run K, a hairline, the state word), then the run's EXISTING
//     content (`ThreadRunBody`: status row + thread / turns for the owner,
//     the status row for a teammate); an ENDED run folds to its name +
//     Show work, which opens that content in place;
//   * the issues its agent linked, one row each under the block (`Linked` /
//     `Filed and linked` + the chip, which opens that issue's child);
//   * a `parallel` block: `Runs 3 and 4 · started by Run 2`, then ONE card
//     with a row per run (mark, name, its issue, `device · age`) that opens
//     the run's page.

function markState(state: ThreadRunState, busy: boolean): RunMarkState | undefined {
  if (state === `ended`) return `ended`
  if (state === `needs_input`) return `needs_input`
  return busy ? `working` : undefined
}

export function ThreadConversation({
  bundle,
  currentUserId,
  now,
  onOpenIssue,
  onOpenRun,
}: {
  bundle: ThreadBundle
  currentUserId: string
  now: Date
  /** A link row's chip / a parallel row's issue: that issue's child. */
  onOpenIssue: (issueId: string) => void
  /** A session's own page (the block header, a parallel card row). */
  onOpenRun: (runId: string) => void
}) {
  const runsById = new Map(bundle.runs.map((run) => [run.id, run]))
  const issuesById = new Map(bundle.issues.map((issue) => [issue.id, issue]))
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set())

  const links = (rows: readonly ThreadLinkRowModel[]) =>
    rows.map((link) => {
      const issue = issuesById.get(link.issueId)
      return (
        <ThreadLinkRow
          key={link.itemId}
          lead={<ExponentialLogo variant="light" size={14} className="size-3.5" />}
          label={link.label}
          chip={
            issue ? (
              <IssueChip
                issue={issue}
                onClick={() => onOpenIssue(issue.id)}
                testId={`thread-link-chip-${issue.identifier}`}
                className="max-w-full"
              />
            ) : (
              <span className="font-mono text-xs">{link.identifier}</span>
            )
          }
        />
      )
    })

  const renderBlock = (block: ThreadBlock) => {
    if (block.kind === `parallel`) {
      const lead = block.runs[0] ? runsById.get(block.runs[0].runId) : undefined
      return (
        <section key={`parallel-${block.runs[0]?.runId}`} className="space-y-2" data-testid="thread-block-parallel">
          <ThreadRunHeader
            lead={<AgentRunMark agent={lead?.agent} state={markState(block.state, false)} ringClassName="ring-background" />}
            label={block.label}
            caption={block.caption}
            state={block.state}
          />
          <ThreadParallelCard
            rows={block.runs.map((row) => {
              const run = runsById.get(row.runId)
              const issue = row.issueId ? issuesById.get(row.issueId) : undefined
              const age = compactRelativeTime(row.startedAt, now.getTime())
              return {
                key: row.runId,
                lead: (
                  <AgentRunMark
                    agent={run?.agent}
                    state={markState(row.state, run?.agentBusy === true)}
                    ringClassName="ring-card"
                  />
                ),
                name: row.name,
                chip: issue ? <IssueChip issue={issue} size="sm" preview={false} /> : undefined,
                meta: [row.deviceLabel, age].filter(Boolean).join(` · `),
                onOpen: () => onOpenRun(row.runId),
              }
            })}
          />
          {links(block.links)}
        </section>
      )
    }
    const run = runsById.get(block.runId)
    if (!run) return null
    const open = !block.folded || expanded.has(block.runId)
    const issue = run.issueId ? issuesById.get(run.issueId) : undefined
    return (
      <section key={block.runId} className="space-y-2" data-testid="thread-block-run" data-run-id={block.runId}>
        <ThreadRunHeader
          lead={
            <AgentRunMark
              agent={run.agent}
              state={markState(block.state, run.agentBusy === true)}
              ringClassName="ring-background"
            />
          }
          label={block.label}
          caption={block.caption}
          state={block.state}
          onOpen={() => onOpenRun(run.id)}
        />
        {open ? (
          <ThreadRunBody
            session={run}
            currentUserId={currentUserId}
            identity={sessionIdentity({ session: run, issue })}
            prState={issue?.prState ?? run.prState}
            // A folded block's Show work opens on the transcript, in place.
            initialShowWork={block.folded}
            threadResults={bundle.thread.results}
            firstSessionId={bundle.view.runIds[0] ?? null}
            className="pl-[1.375rem]"
          />
        ) : (
          <ThreadFoldedRun
            name={bundle.runNames.get(run.id) ?? ``}
            onShowWork={() =>
              setExpanded((current) => new Set([...current, block.runId]))
            }
          />
        )}
        {links(block.links)}
      </section>
    )
  }

  return (
    <div className="flex flex-col gap-6" data-testid="thread-conversation">
      {bundle.view.conversation.map(renderBlock)}
    </div>
  )
}
