/* ─── Board — control row + grouped issue list ───
   Mirrors apps/web boards/$boardSlug/index.tsx (the empty h-14 control row)
   and issue-list.tsx (the md grid
   [checkbox | priority | id | status | title | labels | assignee | due],
   sticky status-washed group headers). */
import {
  GROUP_ORDER,
  ISSUES,
  MY_ISSUE_IDS,
  STATUS_LABEL,
  type Issue,
} from "../ide/data"
import { useWeb } from "./state"
import { LabelPill, PriorityGlyph, StatusGlyph, WebAvatar } from "./bits"
import { WEB_GROUP_ORDER } from "./data"
import { ICON_3, ICON_35, IcCalendar, IcChevRight } from "./icons"

/* The demo board's status groups in the app's display order. GROUP_ORDER
   (ide/data) supplies the labels; contract displayOrder supplies the order. */
const GROUPS = WEB_GROUP_ORDER.map((status) => ({
  status,
  label: GROUP_ORDER.find((g) => g.status === status)?.label ?? STATUS_LABEL[status],
}))

export function WebIssueRow({
  issue,
  entering,
}: {
  issue: Issue
  /* Scene-injected row (EXP-602): plays the is-new entrance animation. */
  entering?: boolean
}) {
  const { openIssue, interactive } = useWeb()
  return (
    <div
      className={`web-row${interactive ? ` is-click` : ``}${entering ? ` is-new` : ``}`}
      onClick={interactive ? () => openIssue(issue.id) : undefined}
    >
      {/* Bulk-select slot — the checkbox only fades in on row hover. */}
      <span className="web-row-check">
        <span className="web-check" />
      </span>
      <span className="web-row-cell">
        <PriorityGlyph priority={issue.priority} />
      </span>
      <span className="web-row-id">{issue.id}</span>
      <span className="web-row-cell">
        <StatusGlyph status={issue.status} />
      </span>
      <span className="web-row-title">{issue.title}</span>
      <span className="web-row-labels">
        {issue.labels?.map((l) => <LabelPill key={l.name} label={l} />)}
      </span>
      <span className="web-row-cell is-center">
        <WebAvatar person={issue.assignee} />
      </span>
      <span className="web-row-due">
        {issue.due && (
          <span className="web-due">
            <IcCalendar size={ICON_3} />
            <span className="web-due-text">{issue.due}</span>
          </span>
        )}
      </span>
    </div>
  )
}

export function WebGroupedList({ issues }: { issues: Issue[] }) {
  const { collapsedGroups, toggleGroup, interactive, injectedIssue } = useWeb()
  return (
    <div className="web-board-list">
      {GROUPS.map((g) => {
        const groupIssues = issues.filter((i) => i.status === g.status)
        if (groupIssues.length === 0) return null
        const isCollapsed = collapsedGroups.has(g.status)
        return (
          <div key={g.status}>
            <div
              className={`web-grouphead web-wash-${g.status}${interactive ? ` is-click` : ``}`}
              onClick={interactive ? () => toggleGroup(g.status) : undefined}
            >
              <span className={`web-groupchev${isCollapsed ? `` : ` is-open`}`}>
                <IcChevRight size={ICON_3} />
              </span>
              <StatusGlyph status={g.status} size={ICON_35} />
              <span className="web-group-name">{g.label}</span>
              <span className="web-group-count">{groupIssues.length}</span>
            </div>
            {!isCollapsed &&
              groupIssues.map((i) => (
                <WebIssueRow
                  key={i.id}
                  issue={i}
                  entering={i.id === injectedIssue?.id}
                />
              ))}
          </div>
        )
      })}
    </div>
  )
}

/* The board page's control row (boards/$boardSlug/index.tsx): a fixed h-14
   band that only fills when rows are selected (the bulk action bar). Issue
   filtering is gone (EXP-862), and with it the Filter trigger. */
export function WebFilterBar() {
  return <div className="web-filterbar" />
}

export function WebBoard() {
  const { injectedIssue } = useWeb()
  /* The injected row leads the list, so it lands FIRST in its status group. */
  const source = injectedIssue ? [injectedIssue, ...ISSUES] : ISSUES
  return (
    <div className="web-page">
      <WebFilterBar />
      <WebGroupedList issues={source} />
    </div>
  )
}

/* "My Issues" — the cross-board assigned list. Lives as a TAB of the Inbox
   page (EXP-186), so it renders no header of its own. */
export function WebMyIssues() {
  const mine = ISSUES.filter((i) => MY_ISSUE_IDS.includes(i.id))
  return <WebGroupedList issues={mine} />
}
