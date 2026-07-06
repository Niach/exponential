/* ─── Source Control: branches sidebar + center tab (changes, commit box, history, diff) ─── */
import { useState } from "react"
import { BRANCHES, type Change } from "./data"
import { useIde } from "./state"
import { ToolHead } from "./bits"
import { DiffView } from "./Diff"
import { IcCheck, IcGitMerge } from "./icons"

export function ScPanel() {
  const { viewedBranch, viewBranch, interactive } = useIde()
  return (
    <div className="ide-scpanel">
      <ToolHead icon={<IcGitMerge size={14} className="ide-c-muted" />} title="Source Control" />
      <div className="ide-sc-branches">
        <div className="ide-sc-label">Branches</div>
        {BRANCHES.map((b) => (
          <div
            key={b.name}
            className={`ide-branch-row${interactive ? ` is-click` : ``}${viewedBranch === b.name ? ` is-viewing` : ``}`}
            onClick={interactive ? () => viewBranch(b.name) : undefined}
          >
            <span className={`ide-branch-glyph${b.current ? `` : ` ide-c-muted`}`}>⎇</span>
            <span className={`ide-branch-name${b.current ? ` is-current` : ``}`}>{b.name}</span>
            {b.worktree && <span className="ide-branch-tag">worktree</span>}
            <div className="ide-flex1" />
            {b.current && <IcCheck size={14} className="ide-c-muted" />}
          </div>
        ))}
      </div>
    </div>
  )
}

function ChangeRow({ change, checked }: { change: Change; checked: boolean }) {
  const { toggleStaged, interactive } = useIde()
  return (
    <div
      className={`ide-change${interactive ? ` is-click` : ``}`}
      onClick={interactive ? () => toggleStaged(change.path) : undefined}
    >
      <span className={`ide-checkbox${checked ? ` is-on` : ``}`}>
        {checked && <IcCheck size={10} />}
      </span>
      <span className={`ide-git-letter ide-change-letter ide-git-${change.status}`}>
        {change.status}
      </span>
      <span className="ide-change-path">{change.path}</span>
    </div>
  )
}

export function ScTab() {
  const { changes, staged, commits, commitAll, interactive } = useIde()
  const [message, setMessage] = useState(``)
  const stagedList = changes.filter((c) => staged.has(c.path))
  const unstagedList = changes.filter((c) => !staged.has(c.path))
  const canCommit = interactive && changes.length > 0 && message.trim().length > 0

  const doCommit = (push: boolean) => {
    if (!canCommit) return
    commitAll(message.trim(), push)
    setMessage(``)
  }

  return (
    <div className="ide-sc">
      <div className="ide-sc-left">
        <div className="ide-sc-changes">
          {stagedList.length > 0 && (
            <>
              <div className="ide-sc-label">{`Staged (${stagedList.length})`}</div>
              {stagedList.map((c) => (
                <ChangeRow key={c.path} change={c} checked />
              ))}
            </>
          )}
          <div className="ide-sc-label">{`Changes (${unstagedList.length})`}</div>
          {unstagedList.map((c) => (
            <ChangeRow key={c.path} change={c} checked={false} />
          ))}
          {changes.length === 0 && <div className="ide-sc-clean">No local changes</div>}
        </div>
        <div className="ide-commitbox">
          <textarea
            className="ide-commitmsg"
            rows={3}
            placeholder="Commit message…"
            value={message}
            readOnly={!interactive}
            onChange={(e) => setMessage(e.target.value)}
          />
          <div className="ide-commit-actions">
            <button
              className={`ide-btn-sm ide-btn-plain${canCommit ? ` is-click` : ``}`}
              type="button"
              disabled={!canCommit}
              onClick={interactive ? () => doCommit(false) : undefined}
            >
              Commit
            </button>
            <button
              className={`ide-btn-sm ide-btn-primary${canCommit ? ` is-click` : ``}`}
              type="button"
              disabled={!canCommit}
              onClick={interactive ? () => doCommit(true) : undefined}
            >
              Commit &amp; Push
            </button>
          </div>
        </div>
        <div className="ide-sc-history">
          <div className="ide-sc-label">History</div>
          {commits.map((c, i) => (
            <div key={i} className="ide-commit">
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
        {changes.length > 0 ? (
          <DiffView />
        ) : (
          <div className="ide-diff-empty">No local changes</div>
        )}
      </div>
    </div>
  )
}
