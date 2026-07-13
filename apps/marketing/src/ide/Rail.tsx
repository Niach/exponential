/* ─── 44px left icon rail with active-tool accent bar ─── */
import { REVIEWS } from "./data"
import { useIde } from "./state"
import {
  IcCircleUser,
  IcFolder,
  IcGitMerge,
  IcGitPullRequest,
  IcInbox,
  IcListTodo,
  IcRocket,
  IcSearch,
  IcSettings,
  type IdeIcon,
} from "./icons"

function RailBtn({
  Icon,
  title,
  active,
  dot,
  onClick,
}: {
  Icon: IdeIcon
  title: string
  active?: boolean
  dot?: boolean
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
      {dot ? <span className="ide-rail-dot" /> : null}
    </button>
  )
}

export function Rail() {
  const { tool, setTool, openSourceControl, interactive, goneReviews } = useIde()
  const on = (fn: () => void) => (interactive ? fn : undefined)
  const openReviews = REVIEWS.filter((r) => !goneReviews.has(r.issueId)).length
  return (
    <div className="ide-rail">
      <RailBtn Icon={IcSearch} title="Search" />
      <div className="ide-rail-div" />
      <RailBtn
        Icon={IcInbox}
        title="Inbox"
        active={tool === `inbox`}
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
        dot={openReviews > 0}
        onClick={on(() => setTool(`reviews`))}
      />
      <RailBtn
        Icon={IcRocket}
        title="Releases"
        active={tool === `releases`}
        onClick={on(() => setTool(`releases`))}
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
