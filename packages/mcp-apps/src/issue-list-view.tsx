import { useState } from "react"
import {
  BUILTIN_PRIORITY_COLOR_CLASS,
  BUILTIN_STATUS_COLOR_CLASS,
  BUILTIN_STATUS_WASH_CLASS,
  EmptyState,
  IssueGroupBand,
  StatusGlyph,
  UserAvatar,
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
import { useAssignees } from "./list-assignees"
import {
  DUE_DATE_TONE_CLASS,
  dueDateTone,
  formatDueDate,
  issueListColumns,
  localDay,
  type IssueListRow,
  type MemberRow,
} from "./list-issue"

const IssuesIcon = conceptIcon(`nav-issues`)
const DueDateIcon = conceptIcon(`ui-due-date`)
const AvatarPlaceholderIcon = conceptIcon(`ui-avatar-placeholder`)

// EXP-1183 — `exponential_issues_list` as the board list: the web's group
// band (`IssueGroupBand`, status glyph + name + count over the status wash)
// over flat hairline rows in the web row's md+ column order — priority,
// identifier, status, title, the PR, the assignee avatar (names resolved
// through the host, `list-assignees.ts`) and the due date in its REV2-48
// tone; a column nobody in the list fills collapses. Read-only: a row opens
// the issue (the host calls `exponential_issues_get` for it).
export function IssueListView({
  issues,
  onOpen,
}: {
  issues: readonly IssueRow[]
  onOpen?: (issue: IssueRow) => void
}) {
  const groups = groupIssuesByStatus(issues)
  const rows = issues as readonly IssueListRow[]
  const columns = issueListColumns(rows)
  const members = useAssignees(rows)
  const today = localDay()
  const gridTemplateColumns = [
    `1.25rem`,
    `4.5rem`,
    `1.25rem`,
    `minmax(0,1fr)`,
    columns.pr ? `auto` : null,
    columns.assignee ? `1.75rem` : null,
    columns.due ? `4.5rem` : null,
  ]
    .filter(Boolean)
    .join(` `)
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
              (group.issues as IssueListRow[]).map((issue) => (
                <IssueLine
                  key={issue.id}
                  issue={issue}
                  columns={columns}
                  gridTemplateColumns={gridTemplateColumns}
                  assignee={
                    issue.assigneeId
                      ? (members.get(issue.assigneeId) ?? { id: issue.assigneeId })
                      : null
                  }
                  today={today}
                  onOpen={onOpen}
                />
              ))}
          </div>
        )
      })}
    </div>
  )
}

function IssueLine({
  issue,
  columns,
  gridTemplateColumns,
  assignee,
  today,
  onOpen,
}: {
  issue: IssueListRow
  columns: ReturnType<typeof issueListColumns>
  gridTemplateColumns: string
  assignee: MemberRow | null
  today: string
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
      style={{ gridTemplateColumns }}
      className="grid h-10 cursor-pointer items-center gap-x-2 border-b border-border/30 px-4 outline-none hover:bg-glass-row focus-visible:bg-glass-row"
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
      {columns.pr && (
        <span className="flex items-center gap-1 text-xs text-muted-foreground">
          {PrIcon && issue.prNumber != null && (
            <>
              <PrIcon className="size-3.5" />#{issue.prNumber}
            </>
          )}
        </span>
      )}
      {columns.assignee && (
        <span
          className="flex items-center justify-center"
          title={assignee ? assignee.name || assignee.email || undefined : `Unassigned`}
        >
          {assignee ? (
            <UserAvatar size={20} user={assignee} />
          ) : (
            <span className="flex size-5 items-center justify-center rounded-full border border-dashed border-border">
              <AvatarPlaceholderIcon className="size-2.5 text-muted-foreground/50" />
            </span>
          )}
        </span>
      )}
      {columns.due && (
        <span className="flex items-center justify-end">
          {issue.dueDate && (
            <span
              className={`flex items-center gap-1 px-1 ${DUE_DATE_TONE_CLASS[dueDateTone(issue.dueDate, today)]}`}
            >
              <DueDateIcon className="size-3 shrink-0" />
              <span className="whitespace-nowrap text-xs">{formatDueDate(issue.dueDate)}</span>
            </span>
          )}
        </span>
      )}
    </div>
  )
}
