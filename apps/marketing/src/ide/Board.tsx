/* ─── All Issues board panel: filter bar, pills, tinted status groups, 28px rows ─── */
import {
  FILTER_STATUSES,
  GROUP_ORDER,
  ISSUES,
  type FilterTab,
  type Issue,
} from "./data"
import { useIde } from "./state"
import { Avatar, LabelChip, PriorityIcon, StatusIcon } from "./bits"
import { IcCalDays, IcChevDown, IcChevRight, IcListFilter, IcPlus } from "./icons"

const PILLS: { id: FilterTab; label: string }[] = [
  { id: `all`, label: `All Issues` },
  { id: `active`, label: `Active` },
  { id: `backlog`, label: `Backlog` },
]

export function IssueRow({ issue }: { issue: Issue }) {
  const { openIssue, interactive, active } = useIde()
  const isOpen = active === `issue:${issue.id}`
  return (
    <div
      className={`ide-row${interactive ? ` is-click` : ``}${isOpen ? ` is-open` : ``}`}
      onClick={interactive ? () => openIssue(issue.id) : undefined}
    >
      <span className="ide-row-cell">
        <PriorityIcon priority={issue.priority} />
      </span>
      <span className="ide-row-id">{issue.id}</span>
      <span className="ide-row-cell">
        <StatusIcon status={issue.status} />
      </span>
      <span className="ide-row-title">{issue.title}</span>
      <span className="ide-row-meta">
        {issue.labels?.map((l) => <LabelChip key={l.name} label={l} />)}
        <Avatar person={issue.assignee} />
        {issue.due ? (
          <span className="ide-due">
            <IcCalDays size={12} />
            {issue.due}
          </span>
        ) : (
          <span className="ide-due is-unset">
            <IcCalDays size={12} />
          </span>
        )}
      </span>
    </div>
  )
}

export function BoardPanel() {
  const { filter, setFilter, collapsedGroups, toggleGroup, interactive } = useIde()
  const visibleStatuses = FILTER_STATUSES[filter]
  return (
    <div className="ide-board">
      <div className="ide-board-top">
        <div className="ide-board-titlerow">
          <span className="ide-board-title">All Issues</span>
          <div className="ide-board-actions">
            <button className="ide-ghost ide-icbtn" type="button" title="Filter">
              <IcListFilter size={14} />
            </button>
            <button className="ide-newissue" type="button">
              <IcPlus size={12} />
              New Issue
            </button>
          </div>
        </div>
        <div className="ide-board-pills">
          {PILLS.map((p) => (
            <button
              key={p.id}
              type="button"
              className={`ide-pill${filter === p.id ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
              onClick={interactive ? () => setFilter(p.id) : undefined}
            >
              {p.label}
            </button>
          ))}
        </div>
      </div>
      <div className="ide-board-list">
        {GROUP_ORDER.filter((g) => visibleStatuses.includes(g.status)).map((g) => {
          const issues = ISSUES.filter((i) => i.status === g.status)
          if (issues.length === 0) return null
          const isCollapsed = collapsedGroups.has(g.status)
          return (
            <div key={g.status}>
              <div
                className={`ide-group ide-group-${g.status}${interactive ? ` is-click` : ``}`}
                onClick={interactive ? () => toggleGroup(g.status) : undefined}
              >
                {isCollapsed ? (
                  <IcChevRight size={14} className="ide-c-muted" />
                ) : (
                  <IcChevDown size={14} className="ide-c-muted" />
                )}
                <StatusIcon status={g.status} />
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
