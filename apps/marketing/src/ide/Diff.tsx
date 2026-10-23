/* ─── THE diff view (diff.rs, EXP-895): UNIFIED, one per-file CARD — the
   header `letter · dimmed dir/basename · +N −M` over 18px rows of
   `old gutter · new gutter · sign · text` in 11px mono, washed with the
   design-tokens `diff` palette. ─── */
import { DIFF_FILE, DIFF_HUNK, DIFF_UNIFIED } from "./data"

export function DiffView() {
  const slash = DIFF_FILE.path.lastIndexOf(`/`)
  const dir = DIFF_FILE.path.slice(0, slash + 1)
  const base = DIFF_FILE.path.slice(slash + 1)
  return (
    <div className="ide-diff">
      <div className="ide-diff-card">
        <div className="ide-diff-head">
          <span className="ide-diff-letter">M</span>
          <span className="ide-diff-path">
            <span className="ide-c-muted">{dir}</span>
            {base}
          </span>
          <span className="ide-flex1" />
          <span className="ide-diff-add">{`+${DIFF_FILE.add}`}</span>
          <span className="ide-diff-del">{`−${DIFF_FILE.del}`}</span>
        </div>
        <div className="ide-diff-hunk">{DIFF_HUNK}</div>
        {DIFF_UNIFIED.map((row, i) => (
          <div key={i} className={`ide-diff-row is-${row.kind}`}>
            <span className="ide-diff-gutter">{row.old ?? ``}</span>
            <span className="ide-diff-gutter">{row.new ?? ``}</span>
            <span className="ide-diff-sign">{row.kind === `add` ? `+` : row.kind === `del` ? `−` : ``}</span>
            <span className="ide-diff-text">{row.text}</span>
          </div>
        ))}
      </div>
    </div>
  )
}
