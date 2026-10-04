import { useState } from "react"
import {
  BUILTIN_PRIORITY_COLOR_CLASS,
  BUILTIN_STATUS_COLOR_CLASS,
  BUILTIN_STATUS_WASH_CLASS,
  EmptyState,
  IssueGroupBand,
  StatusGlyph,
  conceptIcon,
} from "@exp/ui"
import type { IssueStatus } from "@exp/db-schema/domain"
import {
  STATUS_LABEL,
  groupIssuesByStatus,
  priorityIcon,
  prConcept,
  statusIcon,
  type IssueRow,
} from "./model"

const IssuesIcon = conceptIcon(`nav-issues`)

// EXP-1183 — `exponential_issues_list` as the board list: the web's group
// band (`IssueGroupBand`, status glyph + name + count over the status wash)
// over flat hairline rows in the web row's md+ column order — priority,
// identifier, status, title, then the PR state. Read-only: a row opens the
// issue (the host calls `exponential_issues_get` for it).
export function IssueListView({
  issues,
  onOpen,
}: {
  issues: readonly IssueRow[]
  onOpen?: (issue: IssueRow) => void
}) {
  const groups = groupIssuesByStatus(issues)
  const [folded, setFolded] = useState<ReadonlySet<IssueStatus>>(new Set())
  if (groups.length === 0) {
    return (
      <EmptyState
        icon={IssuesIcon}
        title="No issues"
        description="Nothing on these boards matches the query."
      />
    )
  }
  return (
    <div className="flex flex-col">
      {groups.map((group) => {
        const open = !folded.has(group.status)
        return (
          <div key={group.status}>
            <IssueGroupBand
              glyph={{
                icon: statusIcon(group.status),
                colorClass: BUILTIN_STATUS_COLOR_CLASS[group.status],
              }}
              name={STATUS_LABEL[group.status]}
              count={group.issues.length}
              open={open}
              onToggle={() =>
                setFolded((current) => {
                  const next = new Set(current)
                  if (next.has(group.status)) next.delete(group.status)
                  else next.add(group.status)
                  return next
                })
              }
              wash={{ className: BUILTIN_STATUS_WASH_CLASS[group.status] }}
            />
            {open &&
              group.issues.map((issue) => (
                <IssueListRow key={issue.id} issue={issue} onOpen={onOpen} />
              ))}
          </div>
        )
      })}
    </div>
  )
}

function IssueListRow({
  issue,
  onOpen,
}: {
  issue: IssueRow
  onOpen?: (issue: IssueRow) => void
}) {
  const pr = prConcept(issue.prState)
  const PrIcon = pr ? conceptIcon(pr) : null
  return (
    <div
      role="button"
      tabIndex={0}
      data-testid={`issue-row-${issue.identifier}`}
      onClick={() => onOpen?.(issue)}
      onKeyDown={(event) => {
        if (event.key === `Enter` || event.key === ` `) {
          event.preventDefault()
          onOpen?.(issue)
        }
      }}
      className="grid h-10 cursor-pointer grid-cols-[1.25rem_4.5rem_1.25rem_minmax(0,1fr)_auto] items-center gap-x-2 border-b border-border/30 px-4 outline-none hover:bg-glass-row focus-visible:bg-glass-row"
    >
      <StatusGlyph
        icon={priorityIcon(issue.priority)}
        colorClass={BUILTIN_PRIORITY_COLOR_CLASS[issue.priority]}
        className="size-4"
      />
      <span className="truncate font-mono text-xs text-muted-foreground">
        {issue.identifier}
      </span>
      <StatusGlyph
        icon={statusIcon(issue.status)}
        colorClass={BUILTIN_STATUS_COLOR_CLASS[issue.status]}
        className="size-4"
      />
      <span className="truncate text-sm">{issue.title}</span>
      <span className="flex items-center gap-1 text-xs text-muted-foreground">
        {PrIcon && issue.prNumber != null && (
          <>
            <PrIcon className="size-3.5" />#{issue.prNumber}
          </>
        )}
      </span>
    </div>
  )
}
