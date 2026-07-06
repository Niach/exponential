/* ─── Left sidebar panel — switches by active rail tool ─── */
import { useIde } from "./state"
import { BoardPanel } from "./Board"
import { FilesPanel } from "./Files"
import { ScPanel } from "./SourceControl"

export function SidebarPanel() {
  const { tool } = useIde()
  return (
    <div className={`ide-sidebar${tool === `issues` ? ` ide-sidebar-wide` : ``}`}>
      {tool === `issues` ? <BoardPanel /> : tool === `files` ? <FilesPanel /> : <ScPanel />}
    </div>
  )
}
