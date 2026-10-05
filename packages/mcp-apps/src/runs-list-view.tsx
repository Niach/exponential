import { EmptyState, GlassSectionHeader, LiveDot, conceptIcon } from "@exp/ui"
import {
  groupRuns,
  prConcept,
  runIsWorking,
  runStateLabel,
  runSubject,
  runTone,
  type RunDetail,
} from "./model"
import { relativeTime } from "./time"

const RunsIcon = conceptIcon(`coding-running`)

// EXP-1183 — `exponential_sessions_list` as the clients' run list: Running,
// In review and Ended bands (the group band every list wears) over flat
// hairline rows — the state dot, what the run is about, the agent and the
// run's state, the PR. A row opens the run's report (the host calls
// `exponential_sessions_get` for it).
export function RunsListView({
  runs,
  onOpen,
}: {
  runs: readonly RunDetail[]
  onOpen?: (run: RunDetail) => void
}) {
  const groups = groupRuns(runs)
  if (groups.length === 0) {
    return (
      <EmptyState
        icon={RunsIcon}
        title="No runs"
        description="Nothing has run on these teams yet."
      />
    )
  }
  return (
    <div className="flex flex-col">
      {groups.map((group) => (
        <div key={group.label}>
          <GlassSectionHeader label={group.label} count={group.runs.length} />
          {group.runs.map((run) => (
            <RunRow key={run.id} run={run} onOpen={onOpen} />
          ))}
        </div>
      ))}
    </div>
  )
}

function RunRow({
  run,
  onOpen,
}: {
  run: RunDetail
  onOpen?: (run: RunDetail) => void
}) {
  const pr = prConcept(run.prState)
  const PrIcon = pr ? conceptIcon(pr) : null
  const caption = [run.agent, runStateLabel(run), relativeTime(run.createdAt)]
    .filter(Boolean)
    .join(` · `)
  return (
    <div
      role="button"
      tabIndex={0}
      data-testid={`run-row-${run.id}`}
      onClick={() => onOpen?.(run)}
      onKeyDown={(event) => {
        if (event.key === `Enter` || event.key === ` `) {
          event.preventDefault()
          onOpen?.(run)
        }
      }}
      className="flex h-14 cursor-pointer items-center gap-3 border-b border-border/30 px-4 outline-none hover:bg-glass-row focus-visible:bg-glass-row"
    >
      <LiveDot tone={runTone(run)} ping={runIsWorking(run)} className="shrink-0" />
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-sm">{runSubject(run)}</span>
        <span className="truncate text-xs text-muted-foreground">{caption}</span>
      </div>
      {PrIcon && run.prNumber != null && (
        <span className="flex shrink-0 items-center gap-1 text-xs text-muted-foreground">
          <PrIcon className="size-3.5" />#{run.prNumber}
        </span>
      )}
    </div>
  )
}
