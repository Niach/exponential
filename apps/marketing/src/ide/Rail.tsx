/* ─── 44px left icon rail with active-tool accent bar ─── */
import { useIde } from "./state"
import {
  IcCircleUser,
  IcFolder,
  IcGitMerge,
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
  onClick,
}: {
  Icon: IdeIcon
  title: string
  active?: boolean
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
    </button>
  )
}

export function Rail() {
  const { tool, setTool, openSourceControl, interactive } = useIde()
  const on = (fn: () => void) => (interactive ? fn : undefined)
  return (
    <div className="ide-rail">
      <RailBtn Icon={IcSearch} title="Search" />
      <div className="ide-rail-div" />
      <RailBtn Icon={IcInbox} title="Inbox" />
      <RailBtn Icon={IcCircleUser} title="My Issues" />
      <RailBtn
        Icon={IcListTodo}
        title="All Issues"
        active={tool === `issues`}
        onClick={on(() => setTool(`issues`))}
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
