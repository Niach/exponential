/* ─── Work tabs strip (team/work-tabs-strip.tsx, EXP-870/877) ───
   Browser-like tabs on the bare ground above the card, in a 44px band. The
   LIVE runs come first, GROUPED BY AGENT: the brand mark, its chips, then a
   `<` that folds the group down to the mark alone. A live chip leads with the
   run's state dot and has no close button at all; the ordinary tabs follow
   in open order, lead with the issue's status glyph and carry a ×. The
   ACTIVE chip is derived from what the card shows, never stored. */
import { getIssue } from "../ide/data"
import { useWeb } from "./state"
import { AgentMark, RunDot, StatusGlyph } from "./bits"
import { AGENT_LABEL, AGENT_SESSIONS, type DemoAgent } from "./data"
import { ICON_3, ICON_35, IcChevLeft, IcClose } from "./icons"

const AGENT_ORDER: DemoAgent[] = [`claude`, `codex`]

function Chip({ issueId, live }: { issueId: string; live: boolean }) {
  const { interactive, openIssueId, openIssue, closeTab } = useWeb()
  const issue = getIssue(issueId)
  const session = AGENT_SESSIONS.find((s) => s.issueId === issueId)
  const active = openIssueId === issueId
  return (
    <div className={`web-wtab${active ? ` is-active` : ``}`}>
      <button
        type="button"
        className={`web-wtab-main${interactive ? ` is-click` : ``}`}
        onClick={interactive ? () => openIssue(issueId, live ? `run` : `issue`) : undefined}
      >
        <span className="web-wtab-lead">
          {live && session ? (
            <RunDot state={session.state} />
          ) : (
            <StatusGlyph status={issue.status} size={ICON_35} />
          )}
        </span>
        <span className="web-wtab-id">{issue.id}</span>
        <span className="web-wtab-title">{issue.title}</span>
      </button>
      {!live && (
        <button
          type="button"
          className={`web-wtab-close${interactive ? ` is-click` : ``}`}
          aria-label="Close tab"
          onClick={interactive ? () => closeTab(issueId) : undefined}
        >
          <IcClose size={ICON_3} />
        </button>
      )}
    </div>
  )
}

export function WebWorkTabs() {
  const { interactive, tabIds, foldedAgents, toggleAgentFold } = useWeb()
  const liveIds = new Set(AGENT_SESSIONS.map((s) => s.issueId))
  return (
    <div className="web-wtabs">
      {AGENT_ORDER.map((agent) => {
        const runs = AGENT_SESSIONS.filter((s) => s.agent === agent)
        if (runs.length === 0) return null
        const folded = foldedAgents.has(agent)
        return (
          <div className="web-wtab-group" key={agent}>
            <button
              type="button"
              className={`web-wtab-icbtn${interactive ? ` is-click` : ``}`}
              title={`${AGENT_LABEL[agent]} runs`}
              aria-expanded={!folded}
              onClick={interactive ? () => toggleAgentFold(agent) : undefined}
            >
              <AgentMark agent={agent} />
            </button>
            {/* The fold: a grid track sliding 1fr → 0fr, so a folded group's
                chips take no width at all. */}
            <div className={`web-wtab-fold${folded ? ` is-folded` : ``}`}>
              <div className="web-wtab-foldin">
                {runs.map((s) => (
                  <Chip key={s.issueId} issueId={s.issueId} live />
                ))}
                <button
                  type="button"
                  className={`web-wtab-icbtn is-muted${interactive ? ` is-click` : ``}`}
                  title="Collapse"
                  onClick={interactive ? () => toggleAgentFold(agent, true) : undefined}
                >
                  <IcChevLeft size={ICON_35} />
                </button>
              </div>
            </div>
          </div>
        )
      })}
      {tabIds
        .filter((id) => !liveIds.has(id))
        .map((id) => (
          <Chip key={id} issueId={id} live={false} />
        ))}
    </div>
  )
}
