/* ─── Source Control (the footer Computer menu's Source Control,
   source_control.rs, EXP-253/509): a 320px pane inside the content card —
   the "Source Control" head with its refresh glyph, the Uncommitted
   changes row (push and discard glyphs), then the commit graph's two-line
   rows (subject, author · time) — beside the trunk's status bar ("N
   changed file(s) in the working tree. Auto-pull is paused." · View
   changes · Discard changes & reset…) over the picked commit's diff, else
   "Select a commit to view its diff." ─── */
import { useState } from "react"
import { useIde } from "./state"
import { DiffView } from "./Diff"
import { IcRefresh, IcSourceControl, IcTrash, IcUpload } from "./icons"

/* Graph lane colours, in the order the commit graph assigns them. */
const LANE_COLORS = [`#a1a1aa`, `#22c55e`, `#f97316`, `#3b82f6`, `#ef4444`, `#facc15`]

export function ScPanel({ selected, onSelect }: { selected: number; onSelect: (i: number) => void }) {
  const { commits, changes, interactive } = useIde()
  return (
    <div className="ide-pane">
      <div className="ide-pane-head">
        <IcSourceControl size={12.25} className="ide-c-muted" />
        <span className="ide-pane-title">Source Control</span>
        <span className="ide-flex1" />
        <span className="ide-icon-btn" title="Refresh">
          <IcRefresh size={12.25} />
        </span>
      </div>
      <div className="ide-sc-uncommitted">
        <span className="ide-graph-rail is-dotted" />
        <span className="ide-sc-uncommitted-main">
          <span className="ide-sc-uncommitted-title">Uncommitted changes</span>
          <span className="ide-commit-meta">{`${changes.length} file${changes.length === 1 ? `` : `s`}`}</span>
        </span>
        <span className="ide-icon-btn" title="Push">
          <IcUpload size={12.25} />
        </span>
        <span className="ide-icon-btn" title="Discard changes & reset…">
          <IcTrash size={12.25} />
        </span>
      </div>
      <div className="ide-graph">
        {commits.map((c, i) => (
          <div
            key={i}
            className={`ide-graph-row${interactive ? ` is-click` : ``}${i === selected ? ` is-viewing` : ``}`}
            onClick={interactive ? () => onSelect(i) : undefined}
          >
            <span className="ide-graph-rail">
              <span className="ide-graph-dot" style={{ background: LANE_COLORS[i % LANE_COLORS.length] }} />
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

export function ScTab({ selected }: { selected: number }) {
  const { changes, interactive } = useIde()
  const [showChanges, setShowChanges] = useState(false)
  const n = changes.length
  return (
    <div className="ide-sc">
      <div className="ide-sc-bar">
        <span className="ide-sc-bar-text">
          {`${n === 1 ? `1 changed file` : `${n} changed files`} in the working tree. Auto-pull is paused.`}
        </span>
        <span className="ide-flex1" />
        <button
          className={`ide-pill is-sm is-action${interactive ? ` is-click` : ``}`}
          type="button"
          onClick={interactive ? () => setShowChanges((v) => !v) : undefined}
        >
          View changes
        </button>
        <span className="ide-pill is-sm is-action">Discard changes &amp; reset…</span>
      </div>
      {showChanges || selected >= 0 ? (
        <div className="ide-diffpane">
          <DiffView />
        </div>
      ) : (
        <div className="ide-centerhint">Select a commit to view its diff.</div>
      )}
    </div>
  )
}

/* The screen: the pane and the main view share the selection. */
export function SourceControlScreen() {
  const [selected, setSelected] = useState(-1)
  return (
    <>
      <ScPanel selected={selected} onSelect={setSelected} />
      <div className="ide-center">
        <ScTab selected={selected} />
      </div>
    </>
  )
}
