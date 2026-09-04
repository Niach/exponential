/* ─── Source Control: the tool window (working tree + commit graph history)
   and its center changes screen. The IDE is trunk-only and view-only — the
   one write affordance is discard & reset (EXP-253/258/509). ─── */
import { useState } from "react"
import { useIde } from "./state"
import { ToolHead } from "./bits"
import { DiffView } from "./Diff"
import { IcGitMerge, IcRefresh, IcTrash, IcUpload } from "./icons"

/* Graph lane colors, in the order the real commit graph assigns them. */
const LANE_COLORS = [`#a1a1aa`, `#22c55e`, `#f97316`, `#3b82f6`, `#ef4444`, `#facc15`]

export function ScPanel() {
  const { commits, changes, interactive } = useIde()
  const [selected, setSelected] = useState(-1)
  return (
    <div className="ide-scpanel">
      <ToolHead
        icon={<IcGitMerge size={10} className="ide-c-muted" />}
        title="Source Control"
        trailing={
          <span className="ide-icbtn">
            <IcRefresh size={11} />
          </span>
        }
      />
      <div className="ide-sc-uncommitted">
        <div className="ide-sc-uncommitted-main">
          <div className="ide-sc-uncommitted-title">Uncommitted changes</div>
          <div className="ide-sc-uncommitted-sub">
            {`${changes.length} file${changes.length === 1 ? `` : `s`}`}
          </div>
        </div>
        <span className="ide-icbtn">
          <IcUpload size={11} />
        </span>
        <span className="ide-icbtn">
          <IcTrash size={11} />
        </span>
      </div>
      <div className="ide-graph">
        {commits.map((c, i) => (
          <div
            key={i}
            className={`ide-graph-row${interactive ? ` is-click` : ``}${i === selected ? ` is-viewing` : ``}`}
            onClick={interactive ? () => setSelected(i) : undefined}
          >
            <span className="ide-graph-rail">
              <span
                className="ide-graph-dot"
                style={{ background: LANE_COLORS[i % LANE_COLORS.length] }}
              />
            </span>
            <span className="ide-graph-main">
              <span className="ide-commit-subject">{c.subject}</span>
              <span className="ide-commit-meta">{c.meta}</span>
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}

/* The center pane of the Source Control tool: the trunk status bar over the
   selected commit's diff (nothing selected → the hint). */
export function ScTab() {
  const { changes } = useIde()
  const [showDiff, setShowDiff] = useState(false)
  const { interactive } = useIde()
  return (
    <div className="ide-sc">
      <div className="ide-sc-bar">
        <span className="ide-sc-bar-text">
          {`${changes.length} changed file${changes.length === 1 ? `` : `s`} in the working tree. Auto-pull is paused.`}
        </span>
        <div className="ide-flex1" />
        <button
          className={`ide-btn-outline${interactive ? ` is-click` : ``}`}
          type="button"
          onClick={interactive ? () => setShowDiff((v) => !v) : undefined}
        >
          View changes
        </button>
        <button className="ide-btn-outline" type="button">
          Discard changes &amp; reset…
        </button>
      </div>
      {showDiff ? (
        <div className="ide-diffpane">
          <DiffView />
        </div>
      ) : (
        <div className="ide-sc-empty">Select a commit from History to view its diff.</div>
      )}
    </div>
  )
}
