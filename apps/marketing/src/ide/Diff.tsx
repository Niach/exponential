/* ─── Side-by-side diff view: file header, hunk header, 50/50 line rows ─── */
import { DIFF_FILE, DIFF_HUNK, DIFF_ROWS, type DiffCell } from "./data"
import { tintTs } from "./syntax"

function Cell({ cell, left }: { cell: DiffCell; left?: boolean }) {
  const kindCls = cell
    ? cell.kind === `add`
      ? ` is-add`
      : cell.kind === `del`
        ? ` is-del`
        : ``
    : ` is-fill`
  return (
    <div className={`ide-diff-cell${left ? ` is-left` : ``}${kindCls}`}>
      <span className="ide-diff-gutter">{cell ? cell.n : ``}</span>
      <span className="ide-diff-codetext">{cell ? tintTs(cell.text) : ``}</span>
    </div>
  )
}

export function DiffView() {
  return (
    <div className="ide-diff">
      <div className="ide-diff-file">
        <span className="ide-diff-path">{DIFF_FILE.path}</span>
        <div className="ide-flex1" />
        <span className="ide-c-green">{`+${DIFF_FILE.add}`}</span>
        <span className="ide-c-red">{`-${DIFF_FILE.del}`}</span>
      </div>
      <div className="ide-diff-scroll">
        <div className="ide-diff-body">
          <div className="ide-diff-hunk">{DIFF_HUNK}</div>
          {DIFF_ROWS.map((row, i) => (
            <div key={i} className="ide-diff-row">
              <Cell cell={row.l} left />
              <Cell cell={row.r} />
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
