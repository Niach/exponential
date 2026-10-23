/* ─── The board list SCREEN (issue_list.rs, EXP-862/1192): the grouped list
   with nothing above it. A group band (`issue_group_band`, 28px, the status
   hue at 7%) folds its rows; a row is the web grid at compact density —
   the hover checkbox cell · 24px priority · 72px mono identifier · 24px
   status · the title · label pills · the assignee · the due date. A row
   opens its issue as a top tab, ALONE in the card beside the full rail. ─── */
import { GROUP_ORDER, ISSUES, type Issue, type IssueStatus } from "./data"
import { useIde } from "./state"
import { Avatar, LabelChip, PriorityIcon, STATUS_BAND, StatusIcon } from "./bits"
import { IcCalDays, IcChevDown, IcChevRight } from "./icons"

/* Contract display order: backlog · started · completed. */
const DESKTOP_ORDER: IssueStatus[] = [`backlog`, `in_progress`, `in_review`, `done`]

export const GROUPS = DESKTOP_ORDER.map(
  (status) => GROUP_ORDER.find((g) => g.status === status) ?? { status, label: status },
)

export function IssueRow({ issue }: { issue: Issue }) {
  const { openIssue, interactive } = useIde()
  return (
    <div
      className={`ide-row${interactive ? ` is-click` : ``}`}
      onClick={interactive ? () => openIssue(issue.id) : undefined}
    >
      <span className="ide-row-sel" />
      <span className="ide-row-cell">
        <PriorityIcon priority={issue.priority} size={12} />
      </span>
      <span className="ide-row-id">{issue.id}</span>
      <span className="ide-row-cell">
        <StatusIcon status={issue.status} size={12} />
      </span>
      <span className="ide-row-title">{issue.title}</span>
      {issue.labels && issue.labels.length > 0 && (
        <span className="ide-row-labels">
          {issue.labels.map((l) => (
            <LabelChip key={l.name} label={l} />
          ))}
        </span>
      )}
      <span className="ide-row-assignee">
        <Avatar person={issue.assignee} />
      </span>
      {issue.due && (
        <span className="ide-due">
          <IcCalDays size={10.5} />
          {issue.due}
        </span>
      )}
    </div>
  )
}

export function BoardPanel() {
  const { collapsedGroups, toggleGroup, interactive } = useIde()
  return (
    <div className="ide-board">
      {GROUPS.map((g) => {
        const issues = ISSUES.filter((i) => i.status === g.status)
        if (issues.length === 0) return null
        const isCollapsed = collapsedGroups.has(g.status)
        return (
          <div key={g.status}>
            <div
              className={`ide-group${interactive ? ` is-click` : ``}`}
              style={{ background: STATUS_BAND[g.status] }}
              onClick={interactive ? () => toggleGroup(g.status) : undefined}
            >
              <span className="ide-group-chev">
                {isCollapsed ? (
                  <IcChevRight size={10.5} className="ide-c-muted" />
                ) : (
                  <IcChevDown size={10.5} className="ide-c-muted" />
                )}
              </span>
              <StatusIcon status={g.status} size={10.5} />
              <span className="ide-group-label">{g.label}</span>
              <span className="ide-group-count">{issues.length}</span>
            </div>
            {!isCollapsed && issues.map((i) => <IssueRow key={i.id} issue={i} />)}
          </div>
        )
      })}
    </div>
  )
}
