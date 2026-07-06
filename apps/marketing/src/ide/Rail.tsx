/* ─── 44px left icon rail with active-tool accent bar ─── */
import { INBOX_ITEMS, REVIEWS } from "./data"
import { useIde } from "./state"
import {
  IcCircleUser,
  IcFolder,
  IcGitMerge,
  IcGitPullRequest,
  IcInbox,
  IcListTodo,
  IcSearch,
  IcSettings,
  type IdeIcon,
} from "./icons"

function RailBtn({
  Icon,
  title,
  active,
  badge,
  onClick,
}: {
  Icon: IdeIcon
  title: string
  active?: boolean
  badge?: number
  onClick?: () => void
}) {
  return (
    <button
      className={`ide-rail-btn${active ? ` is-active` : ``}${onClick ? ` is-click` : ``}`}
      type="button"
      title={title}
      onClick={onClick}
    >
      <Icon size={16} />
      {badge ? <span className="ide-rail-badge">{badge}</span> : null}
    </button>
  )
}

export function Rail() {
  const { tool, setTool, openSourceControl, interactive, inboxRead, goneReviews } = useIde()
  const on = (fn: () => void) => (interactive ? fn : undefined)
  const unread = INBOX_ITEMS.filter((n) => n.unread && !inboxRead.has(n.id)).length
  const openReviews = REVIEWS.filter((r) => !goneReviews.has(r.issueId)).length
  return (
    <div className="ide-rail">
      <RailBtn Icon={IcSearch} title="Search" />
      <div className="ide-rail-div" />
      <RailBtn
        Icon={IcInbox}
        title="Inbox"
        active={tool === `inbox`}
        badge={unread}
        onClick={on(() => setTool(`inbox`))}
      />
      <RailBtn
        Icon={IcCircleUser}
        title="My Issues"
        active={tool === `my-issues`}
        onClick={on(() => setTool(`my-issues`))}
      />
      <RailBtn
        Icon={IcListTodo}
        title="All Issues"
        active={tool === `issues`}
        onClick={on(() => setTool(`issues`))}
      />
      <RailBtn
        Icon={IcGitPullRequest}
        title="Reviews"
        active={tool === `reviews`}
        badge={openReviews}
        onClick={on(() => setTool(`reviews`))}
      />
      <div className="ide-rail-div" />
      <RailBtn
        Icon={IcFolder}
        title="Files"
        active={tool === `files`}
        onClick={on(() => setTool(`files`))}
      />
      <RailBtn
        Icon={IcGitMerge}
        title="Source Control"
        active={tool === `source-control`}
        onClick={on(openSourceControl)}
      />
      <div className="ide-rail-spacer" />
      <RailBtn Icon={IcSettings} title="Settings" />
      <RailBtn Icon={IcCircleUser} title="Account" />
    </div>
  )
}
