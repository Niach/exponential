import { useState, type ReactNode } from "react"
import { conceptIcon } from "@/lib/icons.generated"
import {
  workflowPhaseCounts,
  type WorkflowAgent,
  type WorkflowAgentState,
  type WorkflowState,
} from "@/lib/agent-feed"
import { formatTokenCount, formatTurnDuration } from "@/lib/working-caption"
import { workflowCaption } from "@exp/domain-contract"
import { cn } from "@/lib/utils"

// EXP-850 §3: the workflow card. A claude `Workflow` tool call is not a tool
// row — it is a small dashboard: what the workflow is, the phases it plans,
// one row per agent with its live telemetry, and the summary it ends with.
// The card REPLACES the `tool` row carrying the same id (the store stamps
// `workflowId` on it), so the call's own settle folds in and no second row is
// ever drawn. Its agents are never subagent tabs and are never steerable
// (§3); a second copy of one of them is the amber warning row below (§4).
//
// Icons are CONCEPTS — this is a multi-client surface (desktop, iOS and
// Android draw the same card).
const CodingSubagentIcon = conceptIcon(`coding-subagent`)
const UiLoadingIcon = conceptIcon(`ui-loading`)
const UiSuccessIcon = conceptIcon(`ui-success`)
const UiErrorIcon = conceptIcon(`ui-error`)
const UiClockIcon = conceptIcon(`ui-clock`)
const UiWarningIcon = conceptIcon(`ui-warning`)
const UiChevronDownIcon = conceptIcon(`ui-chevron-down`)
const UiChevronRightIcon = conceptIcon(`ui-chevron-right`)

const STATE_TONE: Record<WorkflowAgentState, string> = {
  queued: `text-muted-foreground/60`,
  running: `text-foreground`,
  done: `text-emerald-400`,
  error: `text-rose-400`,
}

function AgentStateIcon({ state }: { state: WorkflowAgentState }) {
  const className = cn(`size-3 shrink-0`, STATE_TONE[state])
  if (state === `running`) {
    return <UiLoadingIcon className={cn(className, `animate-spin`)} />
  }
  if (state === `done`) return <UiSuccessIcon className={className} />
  if (state === `error`) return <UiErrorIcon className={className} />
  return <UiClockIcon className={className} />
}

/** The agent's telemetry line — every segment optional, in one order ×4:
 *  model, tokens, tool calls, duration. */
function agentStats(agent: WorkflowAgent): string[] {
  const stats: string[] = []
  if (agent.model) stats.push(agent.model)
  if (agent.tokens !== undefined) {
    stats.push(`${formatTokenCount(agent.tokens)} tokens`)
  }
  if (agent.toolCalls !== undefined && agent.toolCalls > 0) {
    stats.push(`${agent.toolCalls} tool call${agent.toolCalls === 1 ? `` : `s`}`)
  }
  if (agent.durationMs !== undefined && agent.durationMs > 0) {
    stats.push(formatTurnDuration(agent.durationMs))
  }
  return stats
}

/** What the agent's second line SAYS: its error when it failed, its result
 *  preview when it finished, the tool it is on while it runs. */
function agentNote(agent: WorkflowAgent): string | null {
  if (agent.state === `error`) return agent.error ?? null
  if (agent.state === `done`) return agent.resultPreview ?? null
  if (agent.state !== `running`) return null
  const summary = agent.lastToolSummary ?? agent.lastTool
  return summary ?? null
}

/** EXP-856 §4: a second copy of a live agent. Amber, verbatim off the wire —
 *  the sentence is the engine's (`duplicate_agent_detail`), never rebuilt per
 *  client — and never hidden behind a fold. */
export function DuplicateWarningRow({ detail }: { detail: string }) {
  return (
    <div
      className="flex min-w-0 items-start gap-2 text-amber-400"
      data-testid="subagent-duplicate-warning"
    >
      <UiWarningIcon className="mt-0.5 size-3 shrink-0" />
      <span className="min-w-0">{detail}</span>
    </div>
  )
}

function AgentRow({
  agent,
  events,
  renderEvents,
}: {
  agent: WorkflowAgent
  /** The agent's own nested feed rows, when this viewer holds any. */
  events?: ReactNode
  renderEvents: boolean
}) {
  const [expanded, setExpanded] = useState(false)
  const stats = agentStats(agent)
  const note = agentNote(agent)
  const label = agent.label ?? `Agent ${agent.index}`
  const head = (
    <>
      <AgentStateIcon state={agent.state} />
      <span className="min-w-0 truncate font-medium">{label}</span>
      {stats.length > 0 && (
        <span className="min-w-0 truncate text-muted-foreground">
          {stats.join(` · `)}
        </span>
      )}
    </>
  )
  return (
    <div className="min-w-0">
      {renderEvents ? (
        <button
          type="button"
          onClick={() => setExpanded((open) => !open)}
          className="flex w-full min-w-0 items-center gap-1.5 text-left hover:text-foreground"
          aria-expanded={expanded}
        >
          {expanded ? (
            <UiChevronDownIcon className="size-3 shrink-0 text-muted-foreground" />
          ) : (
            <UiChevronRightIcon className="size-3 shrink-0 text-muted-foreground" />
          )}
          {head}
        </button>
      ) : (
        <div className="flex min-w-0 items-center gap-1.5 pl-[calc(0.75rem_+_0.375rem)]">
          {head}
        </div>
      )}
      {note && (
        <div
          className={cn(
            `min-w-0 truncate pl-[calc(0.75rem_+_0.375rem)]`,
            agent.state === `error` ? `text-rose-400/80` : `text-muted-foreground`
          )}
          title={note}
        >
          {note}
        </div>
      )}
      {renderEvents && expanded && <div className="ml-4">{events}</div>}
    </div>
  )
}

export function WorkflowCard({
  workflow,
  agentEvents,
  duplicates = [],
  className,
}: {
  workflow: WorkflowState
  /** The nested rows of a workflow agent, by the agent's wire id — rendered
   *  by the transcript (it owns the row components), folded away here. */
  agentEvents?: Map<string, ReactNode>
  /** §4: the duplicate warnings that named THIS workflow. */
  duplicates?: readonly string[]
  className?: string
}) {
  const phases = workflowPhaseCounts(workflow)
  return (
    <div
      className={cn(
        `min-w-0 space-y-1.5 rounded-md border border-border/60 bg-muted/20 px-3 py-2`,
        className
      )}
      data-testid="workflow-card"
    >
      <div className="flex min-w-0 items-center gap-2">
        <CodingSubagentIcon className="size-3 shrink-0 text-muted-foreground/60" />
        <span className="min-w-0 truncate font-medium">
          {workflowCaption(workflow)}
        </span>
      </div>
      {workflow.description && (
        <div className="min-w-0 text-muted-foreground" title={workflow.description}>
          {workflow.description}
        </div>
      )}
      {phases.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5">
          {phases.map((phase) => (
            <span
              key={phase.index}
              className="flex shrink-0 items-center gap-1 rounded-sm border border-border/60 px-1.5 py-0.5 text-muted-foreground"
              data-testid="workflow-phase"
            >
              <span className="text-foreground">{phase.title}</span>
              {/* The four tallies read in lifecycle order and each one is
                  dropped when it is zero, so a finished phase says "3 done"
                  and nothing else. */}
              {phase.queued > 0 && <span>{`${phase.queued} queued`}</span>}
              {phase.running > 0 && <span>{`${phase.running} running`}</span>}
              {phase.done > 0 && (
                <span className="text-emerald-400">{`${phase.done} done`}</span>
              )}
              {phase.error > 0 && (
                <span className="text-rose-400">{`${phase.error} failed`}</span>
              )}
            </span>
          ))}
        </div>
      )}
      {workflow.agents.length > 0 && (
        <div className="min-w-0 space-y-1">
          {workflow.agents.map((agent) => {
            const events = agent.agentId
              ? agentEvents?.get(agent.agentId)
              : undefined
            return (
              <AgentRow
                key={agent.index}
                agent={agent}
                events={events}
                renderEvents={events !== undefined}
              />
            )
          })}
        </div>
      )}
      {duplicates.map((detail) => (
        <DuplicateWarningRow key={detail} detail={detail} />
      ))}
      {workflow.summary && (
        <div className="min-w-0 border-t border-border/60 pt-1.5 text-muted-foreground">
          {workflow.summary}
        </div>
      )}
    </div>
  )
}
