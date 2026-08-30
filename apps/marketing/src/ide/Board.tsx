/* ─── The board tool window (520px): the right-aligned Filter trigger over a
   grouped 28px-row virtual list. Grid = 24px priority · 72px identifier ·
   24px status · 1fr title · labels · assignee · due (issue_list.rs). ─── */
import { GROUP_ORDER, ISSUES, type IssueStatus, type Issue } from "./data"
import { useIde } from "./state"
import { Avatar, LabelChip, PriorityIcon, StatusIcon } from "./bits"
import { IcCalDays, IcChevDown, IcChevRight, IcListFilter } from "./icons"

/* Contract display order (backlog · started · completed), the
   order the real list groups in — `GROUP_ORDER` keeps the web fixture's. */
const DESKTOP_ORDER: IssueStatus[] = [`backlog`, `in_progress`, `in_review`, `done`]

const GROUPS = DESKTOP_ORDER.map(
  (status) => GROUP_ORDER.find((g) => g.status === status) ?? { status, label: status },
)

export function IssueRow({ issue }: { issue: Issue }) {
  const { openIssue, interactive, active } = useIde()
  const isOpen = active === `issue:${issue.id}`
  return (
    <div
      className={`ide-row${interactive ? ` is-click` : ``}${isOpen ? ` is-open` : ``}`}
      onClick={interactive ? () => openIssue(issue.id) : undefined}
    >
      <span className="ide-row-sel" />
      <span className="ide-row-cell">
        <PriorityIcon priority={issue.priority} size={10} />
      </span>
      <span className="ide-row-id">{issue.id}</span>
      <span className="ide-row-cell">
        <StatusIcon status={issue.status} size={10} />
      </span>
      <span className="ide-row-title">{issue.title}</span>
      <span className="ide-row-labels">
        {issue.labels?.map((l) => <LabelChip key={l.name} label={l} />)}
      </span>
      <span className="ide-row-assignee">
        <Avatar person={issue.assignee} />
      </span>
      {issue.due ? (
        <span className="ide-due">
          <IcCalDays size={10} />
          {issue.due}
        </span>
      ) : null}
    </div>
  )
}

export function BoardPanel() {
  const { collapsedGroups, toggleGroup, interactive } = useIde()
  return (
    <div className="ide-board">
      <div className="ide-filterbar">
        <button className="ide-filterbtn" type="button">
          <IcListFilter size={10} />
          Filter
        </button>
      </div>
      <div className="ide-board-list">
        {GROUPS.map((g) => {
          const issues = ISSUES.filter((i) => i.status === g.status)
          if (issues.length === 0) return null
          const isCollapsed = collapsedGroups.has(g.status)
          return (
            <div key={g.status}>
              <div
                className={`ide-group ide-group-${g.status}${interactive ? ` is-click` : ``}`}
                onClick={interactive ? () => toggleGroup(g.status) : undefined}
              >
                <span className="ide-group-chev">
                  {isCollapsed ? (
                    <IcChevRight size={10} className="ide-c-muted" />
                  ) : (
                    <IcChevDown size={10} className="ide-c-muted" />
                  )}
                </span>
                <StatusIcon status={g.status} size={10} />
                <span className="ide-group-label">{g.label}</span>
                <span className="ide-group-count">{issues.length}</span>
              </div>
              {!isCollapsed && issues.map((i) => <IssueRow key={i.id} issue={i} />)}
            </div>
          )
        })}
      </div>
    </div>
  )
}
