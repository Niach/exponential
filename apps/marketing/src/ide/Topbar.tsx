/* ─── 38px top bar: project pill · run widget · git cluster ─── */
import { PROJECT } from "./data"
import { useIde } from "./state"
import {
  IcArrowDL,
  IcArrowUR,
  IcCheck,
  IcChevDown,
  IcChevsUpDown,
  IcPlay,
} from "./icons"

export function Topbar() {
  const { ahead } = useIde()
  return (
    <div className="ide-topbar">
      <button className="ide-proj" type="button">
        <span className="ide-proj-dot" style={{ background: PROJECT.color }} />
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
        {ahead > 0 && <span className="ide-aheadbehind">{`↑${ahead}`}</span>}
        <button className="ide-ghost ide-icbtn" type="button" title="Commit…">
          <IcCheck size={14} />
        </button>
        <button className="ide-ghost ide-icbtn" type="button" title="Pull">
          <IcArrowDL size={14} />
        </button>
        <button className="ide-ghost ide-icbtn" type="button" title="Push">
          <IcArrowUR size={14} />
        </button>
      </div>
    </div>
  )
}
