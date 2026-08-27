/* ─── The 34px window titlebar right of the rail (EXP-277/288/449): the
   center tab strip plus the solid "New Issue" button at the far right. ─── */
import { getIssue } from "./data"
import { useIde, type Tab } from "./state"
import { StatusIcon } from "./bits"
import { IcExternalLink, IcFile, IcGitMerge, IcPlus, IcX } from "./icons"

function TabChip({ tab }: { tab: Tab }) {
  const { active, selectTab, closeTab, interactive } = useIde()
  const isActive = tab.key === active
  const issue = tab.kind === `issue` ? getIssue(tab.ref) : null
  return (
    <div
      className={`ide-tab${isActive ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
      onClick={interactive ? () => selectTab(tab.key) : undefined}
    >
      {issue ? (
        <StatusIcon status={issue.status} size={10} />
      ) : tab.kind === `file` ? (
        <IcFile size={10} className="ide-c-muted" />
      ) : (
        <IcGitMerge size={10} className="ide-c-muted" />
      )}
      {issue && <span className="ide-tab-id">{issue.id}</span>}
      <span className="ide-tab-title">{issue ? issue.title : tab.label}</span>
      <span className="ide-tab-tools">
        <span className="ide-tab-undock" aria-hidden>
          <IcExternalLink size={10} />
        </span>
        <button
          className={`ide-tab-x${interactive ? ` is-click` : ``}`}
          type="button"
          title="Close tab"
          onClick={
            interactive
              ? (e) => {
                  e.stopPropagation()
                  closeTab(tab.key)
                }
              : undefined
          }
        >
          <IcX size={10} />
        </button>
      </span>
    </div>
  )
}

export function Topbar() {
  const { tabs } = useIde()
  return (
    <div className="ide-titlebar">
      <div className="ide-tabstrip">
        {tabs.map((tab) => (
          <TabChip key={tab.key} tab={tab} />
        ))}
      </div>
      <div className="ide-flex1" />
      <button className="ide-newissue" type="button">
        <IcPlus size={11} />
        New Issue
      </button>
    </div>
  )
}
