/* ─── The work-tabs band (app_title_bar.rs + screens.rs render_tab_strip,
   EXP-877/923/1255): 44px of bare ground above the content card, 8px in
   from the rail. A FLAT strip of what you opened — issues and threads —
   each THE strip chip (surface.rs `rich_tab`: 32px, 6px radius, the row
   hairline at rest, the strong stroke over the glass ACTIVE fill when up):
   a 14px lead (an issue's status glyph, a thread's session-state glyph),
   the mono identifier, the title capped at 180px, and the × that shows on
   hover only. The ghost `+` (New session) closes the strip, tabs or not. ─── */
import { getIssue } from "./data"
import { useIde, type Tab, type ThreadState } from "./state"
import { StatusIcon } from "./bits"
import { IcAlert, IcAssistant, IcCircleCheck, IcPlay, IcPlus, IcX } from "./icons"

/* session-card.json `icons` + `tones`: the thread chip's lead. */
export function SessionStateGlyph({ state, size = 14 }: { state: ThreadState; size?: number }) {
  switch (state) {
    case `working`:
      return <IcPlay size={size} className="ide-c-fg" />
    case `needs_you`:
      return <IcAlert size={size} className="ide-c-amber" />
    case `waiting`:
      return <IcAssistant size={size} className="ide-c-muted" />
    case `ended`:
      return <IcCircleCheck size={size} className="ide-c-muted" />
  }
}

function TabChip({ tab }: { tab: Tab }) {
  const { active, selectTab, closeTab, interactive, thread } = useIde()
  const isActive = tab.key === active
  const issue = tab.kind === `issue` ? getIssue(tab.ref) : null
  const title = issue ? issue.title : thread ? `#${thread.issueId} ${getIssue(thread.issueId).title}` : ``
  return (
    <div
      className={`ide-tab${isActive ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
      onClick={interactive ? () => selectTab(tab.key) : undefined}
      title={issue ? `${issue.id} ${issue.title}` : title}
    >
      <span className="ide-tab-lead">
        {issue ? (
          <StatusIcon status={issue.status} size={14} />
        ) : thread ? (
          <SessionStateGlyph state={thread.state} />
        ) : null}
      </span>
      {issue && <span className="ide-tab-id">{issue.id}</span>}
      <span className="ide-tab-title">{title}</span>
      <button
        className={`ide-tab-x${interactive ? ` is-click` : ``}`}
        type="button"
        title="Close"
        aria-label="Close"
        onClick={
          interactive
            ? (e) => {
                e.stopPropagation()
                closeTab(tab.key)
              }
            : undefined
        }
      >
        <IcX size={10.5} />
      </button>
    </div>
  )
}

export function Topbar() {
  const { tabs, interactive, setTool } = useIde()
  return (
    <div className="ide-titlebar">
      <div className="ide-tabstrip">
        {tabs.map((tab) => (
          <TabChip key={tab.key} tab={tab} />
        ))}
        {/* EXP-1294: the `+` opens Home ("New session"). */}
        <button
          className={`ide-strip-new${interactive ? ` is-click` : ``}`}
          type="button"
          title="New session"
          aria-label="New session"
          onClick={interactive ? () => setTool(`agent`) : undefined}
        >
          <IcPlus size={10.5} />
        </button>
      </div>
    </div>
  )
}
