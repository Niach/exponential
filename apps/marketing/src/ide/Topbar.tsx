/* ─── 38px top bar: board pill · run widget · git cluster ─── */
import { PROJECT } from "./data"
import { useIde } from "./state"
import { IcCheck, IcChevDown, IcChevsUpDown, IcCode, IcPlay } from "./icons"

export function Topbar() {
  const { ahead, push, interactive } = useIde()
  return (
    <div className="ide-topbar">
      <button className="ide-proj" type="button">
        {/* Board glyph, tinted with the board color. */}
        <IcCode size={14} style={{ color: PROJECT.color }} />
        <span className="ide-proj-name">{PROJECT.name}</span>
        <IcChevsUpDown size={12} className="ide-c-muted" />
      </button>
      <div className="ide-flex1" />
      <button className="ide-ghost ide-runcfg" type="button" title="Run configuration">
        Dev Server
        <IcChevDown size={12} />
      </button>
      <button className="ide-ghost ide-icbtn" type="button" title="Run">
        <IcPlay size={14} className="ide-c-green" />
      </button>
      <div className="ide-vdiv" />
      <div className="ide-gitcluster">
        <button className="ide-ghost ide-branchbtn" type="button" title="Branches">
          <span className="ide-branch-glyph">⎇</span>
          master
        </button>
        <button className="ide-ghost ide-icbtn" type="button" title="Commit…">
          <IcCheck size={14} />
        </button>
        {/* One context-sensitive action — the count IS the button. A clean,
            in-sync trunk renders nothing. */}
        {ahead > 0 && (
          <button
            className={`ide-ghost ide-syncbtn${interactive ? ` is-click` : ``}`}
            type="button"
            title="Push master to origin"
            onClick={interactive ? push : undefined}
          >
            {`↑${ahead}`}
          </button>
        )}
      </div>
    </div>
  )
}
