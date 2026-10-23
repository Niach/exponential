/* ─── Work tabs strip (team/work-tabs-strip.tsx, EXP-870/1255/1334) ───
   Browser-like tabs on the bare ground above the card, in a 44px band: a
   FLAT chip list in open order (an issue chip = its status glyph + the mono
   identifier + the title), every chip THE strip chip (`@exp/ui` StripChip:
   h-8, rounded-md, bordered at rest), its × shown only while the tab is
   hovered; a ghost "+" (New session) after the last tab. The ACTIVE chip is
   derived from what the card shows, never stored. With no tab open the band
   holds the "+" alone (shots/board/web). */
import { getIssue } from "../ide/data"
import { useWeb } from "./state"
import { StatusGlyph } from "./bits"
import { ICON_3, ICON_35, IcClose, IcPlus } from "./icons"

function Chip({ issueId }: { issueId: string }) {
  const { interactive, openIssueId, openIssue, closeTab } = useWeb()
  const issue = getIssue(issueId)
  const active = openIssueId === issueId
  return (
    <div className={`web-wtab${active ? ` is-active` : ``}`}>
      <button
        type="button"
        className={`web-wtab-main${interactive ? ` is-click` : ``}`}
        onClick={interactive ? () => openIssue(issueId) : undefined}
      >
        <span className="web-wtab-lead">
          <StatusGlyph status={issue.status} size={ICON_35} />
        </span>
        <span className="web-wtab-id">{issue.id}</span>
        <span className="web-wtab-title">{issue.title}</span>
      </button>
      <button
        type="button"
        className={`web-wtab-close${interactive ? ` is-click` : ``}`}
        aria-label="Close tab"
        onClick={interactive ? () => closeTab(issueId) : undefined}
      >
        <IcClose size={ICON_3} />
      </button>
    </div>
  )
}

export function WebWorkTabs() {
  const { tabIds } = useWeb()
  return (
    <div className="web-wtabs">
      {tabIds.map((id) => (
        <Chip key={id} issueId={id} />
      ))}
      <button type="button" className="web-wtab-new" title="New session">
        <IcPlus size={ICON_3} />
      </button>
    </div>
  )
}
