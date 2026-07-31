/* ─── Source Control: branch lanes sidebar + center tab (commit history +
   diff). The IDE is view-only — it neither stages, commits nor pushes; the
   one write affordance is discard & reset (EXP-253/258). ─── */
import { useState } from "react"
import { LANES, type Change, type Lane } from "./data"
import { useIde } from "./state"
import { ToolHead } from "./bits"
import { DiffView } from "./Diff"
import { IcCheck, IcCircleCheck, IcGitMerge, IcTimer } from "./icons"

/* Lane indicator: merged = green check, open PR = green dot, local work
   with no PR = yellow in-progress badge; the default branch carries none. */
function LaneIndicator({ lane }: { lane: Lane }) {
  if (lane.indicator === `merged`) return <IcCircleCheck size={13} className="ide-c-green" />
  if (lane.indicator === `open`) return <span className="ide-lane-dot is-open" />
  if (lane.indicator === `progress`) return <IcTimer size={13} className="ide-c-yellow" />
  return null
}

export function ScPanel() {
  const { viewedBranch, viewBranch, interactive } = useIde()
  return (
    <div className="ide-scpanel">
      <ToolHead icon={<IcGitMerge size={14} className="ide-c-muted" />} title="Source Control" />
      <div className="ide-sc-branches">
        {LANES.map((lane) => (
          <div
            key={lane.branch}
            className={`ide-lane${interactive ? ` is-click` : ``}${viewedBranch === lane.branch ? ` is-viewing` : ``}`}
            onClick={interactive ? () => viewBranch(lane.branch) : undefined}
          >
            {lane.indent > 0 && <span className="ide-lane-elbow" />}
            {lane.indent === 0 ? (
              <span className="ide-branch-glyph">⎇</span>
            ) : (
              <LaneIndicator lane={lane} />
            )}
            <span className={`ide-branch-name${lane.current ? ` is-current` : ``}`}>
              {lane.branch}
            </span>
            {lane.worktree && <span className="ide-branch-tag">worktree</span>}
            <div className="ide-flex1" />
            {(lane.behind ?? 0) > 0 && (
              <span className="ide-lane-counts">{`↓${lane.behind}`}</span>
            )}
            {(lane.ahead ?? 0) > 0 && <span className="ide-lane-counts">{`↑${lane.ahead}`}</span>}
            {lane.current && <IcCheck size={14} className="ide-c-muted" />}
          </div>
        ))}
      </div>
    </div>
  )
}

/* The trunk is expected clean, so a dirty tree is an ANOMALY (EXP-258):
   a slim count strip with the one write affordance — discard & reset. */
function DirtyStrip({ changes }: { changes: Change[] }) {
  if (changes.length === 0) return null
  return (
    <div className="ide-sc-dirty">
      <span className="ide-sc-dirty-count">
        {`${changes.length} local change${changes.length === 1 ? `` : `s`}`}
      </span>
      <div className="ide-flex1" />
      <button className="ide-btn-sm ide-btn-plain" type="button">
        Discard
      </button>
    </div>
  )
}

export function ScTab() {
  const { changes, commits } = useIde()
  const [selected, setSelected] = useState(0)

  return (
    <div className="ide-sc">
      <div className="ide-sc-left">
        <DirtyStrip changes={changes} />
        <div className="ide-sc-history">
          <div className="ide-sc-label">History</div>
          {commits.map((c, i) => (
            <div
              key={i}
              className={`ide-commit${i === selected ? ` is-viewing` : ``}`}
              onClick={() => setSelected(i)}
            >
              <div className="ide-commit-subject">{c.subject}</div>
              <div className="ide-commit-meta">{c.meta}</div>
            </div>
          ))}
          <button className="ide-ghost ide-loadmore" type="button">
            Load more
          </button>
        </div>
      </div>
      <div className="ide-diffpane">
        <DiffView />
      </div>
    </div>
  )
}
