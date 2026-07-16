/* ─── Project board — filter bar + grouped issue list ───
   Mirrors apps/web issue-filter-bar.tsx ("Issues" title, pill tabs, indigo
   New Issue) and issue-list.tsx (grid rows [priority | identifier | status |
   title | labels | assignee | due], tinted sticky group headers). */
import {
  FILTER_STATUSES,
  GROUP_ORDER,
  ISSUES,
  MY_ISSUE_IDS,
  type FilterTab,
  type Issue,
} from "../ide/data"
import { useWeb } from "./state"
import { Avatar, LabelChip, PriorityIcon, StatusIcon } from "../ide/bits"
import { IcCalDays, IcChevRight, IcCircleUser, IcListFilter, IcPlus } from "../ide/icons"

const TABS: { id: FilterTab; label: string }[] = [
  { id: `all`, label: `All Issues` },
  { id: `active`, label: `Active` },
  { id: `backlog`, label: `Backlog` },
]

export function WebIssueRow({ issue }: { issue: Issue }) {
  const { openIssue, interactive } = useWeb()
  return (
    <div
      className={`web-row${interactive ? ` is-click` : ``}`}
      onClick={interactive ? () => openIssue(issue.id) : undefined}
    >
      <span className="web-row-cell">
        <PriorityIcon priority={issue.priority} />
      </span>
      <span className="web-row-id">{issue.id}</span>
      <span className="web-row-cell">
        <StatusIcon status={issue.status} />
      </span>
      <span className="web-row-title">{issue.title}</span>
      <span className="web-row-labels">
        {issue.labels?.map((l) => <LabelChip key={l.name} label={l} />)}
      </span>
      <span className="web-row-cell is-center">
        <Avatar person={issue.assignee} size={18} />
      </span>
      <span className="web-row-due">
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

function GroupedList({ issues }: { issues: Issue[] }) {
  const { collapsedGroups, toggleGroup, interactive } = useWeb()
  return (
    <div className="web-board-list">
      {GROUP_ORDER.map((g) => {
        const groupIssues = issues.filter((i) => i.status === g.status)
        if (groupIssues.length === 0) return null
        const isCollapsed = collapsedGroups.has(g.status)
        return (
          <div key={g.status}>
            <div
              className={`web-grouphead web-grouphead-${g.status}${interactive ? ` is-click` : ``}`}
              onClick={interactive ? () => toggleGroup(g.status) : undefined}
            >
              <span className={`web-groupchev${isCollapsed ? `` : ` is-open`}`}>
                <IcChevRight size={12} className="ide-c-muted" />
              </span>
              <StatusIcon status={g.status} />
              <span className="web-group-name">{g.label}</span>
              <span className="web-group-count">{groupIssues.length}</span>
            </div>
            {!isCollapsed && groupIssues.map((i) => <WebIssueRow key={i.id} issue={i} />)}
          </div>
        )
      })}
    </div>
  )
}

export function WebBoard() {
  const { filter, setFilter, interactive } = useWeb()
  const visibleStatuses = FILTER_STATUSES[filter]
  const visible = ISSUES.filter((i) => visibleStatuses.includes(i.status))
  return (
    <div className="web-board">
      <div className="web-filterbar">
        <div className="web-filterbar-top">
          <span className="web-board-title">Issues</span>
          <div className="web-board-actions">
            <button className="web-ghost web-icbtn" type="button" title="Filter">
              <IcListFilter size={14} />
            </button>
            <button className="web-newissue" type="button">
              <IcPlus size={12} />
              New Issue
            </button>
          </div>
        </div>
        <div className="web-tabs">
          {TABS.map((tab) => (
            <button
              key={tab.id}
              type="button"
              className={`web-tab${filter === tab.id ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
              onClick={interactive ? () => setFilter(tab.id) : undefined}
            >
              {tab.label}
            </button>
          ))}
        </div>
      </div>
      <GroupedList issues={visible} />
    </div>
  )
}

export function WebMyIssues() {
  const mine = ISSUES.filter((i) => MY_ISSUE_IDS.includes(i.id))
  return (
    <div className="web-board">
      <div className="web-filterbar">
        <div className="web-filterbar-top">
          <span className="web-board-title web-board-title-icon">
            <IcCircleUser size={15} className="ide-c-muted" />
            My Issues
          </span>
        </div>
      </div>
      <GroupedList issues={mine} />
    </div>
  )
}
