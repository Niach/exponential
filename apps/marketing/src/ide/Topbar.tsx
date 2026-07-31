/* ─── 38px top bar: board pill · trunk indicator. The IDE is trunk-only and
   view-only (EXP-253/258) — no branch switch, no commit, no push. ─── */
import { PROJECT } from "./data"
import { IcChevsUpDown, IcCode } from "./icons"

export function Topbar() {
  return (
    <div className="ide-topbar">
      <button className="ide-proj" type="button">
        {/* Board glyph, tinted with the board color. */}
        <IcCode size={14} style={{ color: PROJECT.color }} />
        <span className="ide-proj-name">{PROJECT.name}</span>
        <IcChevsUpDown size={12} className="ide-c-muted" />
      </button>
      <div className="ide-flex1" />
      <div className="ide-gitcluster">
        {/* Trunk, kept level with origin by the headless sync — an
            indicator, not a switcher. */}
        <span className="ide-ghost ide-branchbtn" title="Trunk">
          <span className="ide-branch-glyph">⎇</span>
          master
        </span>
      </div>
    </div>
  )
}
