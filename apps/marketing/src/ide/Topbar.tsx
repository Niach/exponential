/* ─── The work-tabs band (app_title_bar.rs + screens.rs render_tab_strip,
   EXP-870/877): 44px of bare ground above the cutout panel, 32px chips
   centred in it. Live runs of mine LEAD the strip, clustered by agent: the
   agent's brand mark, its chips, then a chevron that folds the cluster to
   the mark (the mark unfolds it again). A live run's chip carries its
   state (spinner while its turn runs, amber waiting on you, green PR open)
   and cannot be closed. Other tabs follow with their status glyph and ×.
   The active tab wears the glass ACTIVE fill. ─── */
import { AGENT_LABEL, getIssue, type AgentKind } from "./data"
import { isLive, useIde, type RunView, type Tab } from "./state"
import { StatusIcon } from "./bits"
import { IcChevLeft, IcClaude, IcCodex, IcX } from "./icons"

export function AgentMark({ agent, size = 14 }: { agent: AgentKind; size?: number }) {
  return agent === `claude` ? (
    <IcClaude size={size} />
  ) : (
    <IcCodex size={size} className="ide-c-fg" />
  )
}

/* The ONE session dot (queries.rs SessionDotFacts). */
export function RunDot({ run }: { run: RunView }) {
  if (run.state === `running`) return <span className="ide-rail-spinner" />
  return (
    <span
      className={`ide-rundot${run.state === `waiting` ? ` is-waiting` : run.state === `ended` ? ` is-ended` : ``}`}
    />
  )
}

function TabChip({ tab, run }: { tab: Tab; run: RunView | null }) {
  const { active, selectTab, closeTab, interactive } = useIde()
  const isActive = tab.key === active
  const issue = tab.kind === `issue` ? getIssue(tab.ref) : null
  const closable = !run || !isLive(run.state)
  return (
    <div
      className={`ide-tab${isActive ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
      onClick={interactive ? () => selectTab(tab.key) : undefined}
      title={issue ? `${issue.id} ${issue.title}` : tab.label}
    >
      {run ? <RunDot run={run} /> : issue ? <StatusIcon status={issue.status} size={11} /> : null}
      {issue && <span className="ide-tab-id">{issue.id}</span>}
      <span className="ide-tab-title">{issue ? issue.title : (run?.title ?? tab.label)}</span>
      {closable && (
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
          <IcX size={11} />
        </button>
      )}
    </div>
  )
}

export function Topbar() {
  const { tabs, runForTab, foldedAgents, toggleAgentFold, interactive } = useIde()
  const withRuns = tabs.map((tab) => ({ tab, run: runForTab(tab.key) }))
  const groups = ([`claude`, `codex`] as AgentKind[])
    .map((agent) => ({
      agent,
      items: withRuns.filter(({ run }) => run && isLive(run.state) && run.agent === agent),
    }))
    .filter((g) => g.items.length > 0)
  const rest = withRuns.filter(({ run }) => !run || !isLive(run.state))
  return (
    <div className="ide-titlebar">
      <div className="ide-tabstrip">
        {groups.map(({ agent, items }) => {
          const folded = foldedAgents.has(agent)
          return (
            <div key={agent} className="ide-tabgroup">
              <button
                className={`ide-tabgroup-mark${interactive ? ` is-click` : ``}`}
                type="button"
                title={folded ? `Show ${AGENT_LABEL[agent]} runs` : AGENT_LABEL[agent]}
                onClick={interactive && folded ? () => toggleAgentFold(agent) : undefined}
              >
                <AgentMark agent={agent} />
              </button>
              {!folded && (
                <>
                  {items.map(({ tab, run }) => (
                    <TabChip key={tab.key} tab={tab} run={run} />
                  ))}
                  <button
                    className={`ide-tabgroup-fold${interactive ? ` is-click` : ``}`}
                    type="button"
                    title="Collapse"
                    onClick={interactive ? () => toggleAgentFold(agent) : undefined}
                  >
                    <IcChevLeft size={12} />
                  </button>
                </>
              )}
            </div>
          )
        })}
        {rest.map(({ tab, run }) => (
          <TabChip key={tab.key} tab={tab} run={run} />
        ))}
      </div>
      <div className="ide-flex1" />
    </div>
  )
}
