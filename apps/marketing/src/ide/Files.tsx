/* ─── Files tool window (trunk file tree) + the center file viewer. Files are
   NOT tabs (EXP-288): the viewer IS the Files tool's center content. ─── */
import { Fragment } from "react"
import { FILE_TREE, PACKAGE_JSON, type FileNode } from "./data"
import { useIde } from "./state"
import { ToolHead } from "./bits"
import { tintJson } from "./syntax"
import {
  IcChevDown,
  IcChevRight,
  IcFile,
  IcFolder,
  IcFolderOpen,
  IcGitBranch,
  IcRefresh,
} from "./icons"

function TreeRows({ nodes, depth }: { nodes: FileNode[]; depth: number }) {
  const { expandedDirs, toggleDir, selectedFile, selectFile, interactive } = useIde()
  return (
    <>
      {nodes.map((n) => {
        const isDir = Boolean(n.children)
        const isExpanded = isDir && expandedDirs.has(n.path)
        const isSelected = !isDir && selectedFile === n.path
        const onClick = interactive
          ? () => (isDir ? toggleDir(n.path) : selectFile(n.path))
          : undefined
        return (
          <Fragment key={n.path}>
            <div
              className={`ide-tree-row${interactive ? ` is-click` : ``}${isSelected ? ` is-selected` : ``}${n.dim ? ` is-dim` : ``}`}
              style={{ paddingLeft: 10 + depth * 13 }}
              onClick={onClick}
            >
              {isDir ? (
                isExpanded ? (
                  <IcChevDown size={10} className="ide-c-dim" />
                ) : (
                  <IcChevRight size={10} className="ide-c-dim" />
                )
              ) : (
                <span className="ide-tree-spacer" />
              )}
              {isDir ? (
                isExpanded ? (
                  <IcFolderOpen size={10} className="ide-c-muted" />
                ) : (
                  <IcFolder size={10} className="ide-c-muted" />
                )
              ) : (
                <IcFile size={10} className="ide-c-muted" />
              )}
              <span className="ide-tree-name">{n.name}</span>
              {n.git && <span className={`ide-git-letter ide-git-${n.git}`}>{n.git}</span>}
            </div>
            {isDir && isExpanded && n.children && (
              <TreeRows nodes={n.children} depth={depth + 1} />
            )}
          </Fragment>
        )
      })}
    </>
  )
}

export function FilesPanel() {
  return (
    <div className="ide-filespanel">
      <ToolHead
        icon={<IcFolder size={10} className="ide-c-muted" />}
        title="Files"
        trailing={
          <span className="ide-icbtn">
            <IcRefresh size={11} />
          </span>
        }
      />
      <div className="ide-branchrow">
        <IcGitBranch size={10} className="ide-c-muted" />
        <span>master</span>
        <IcChevDown size={10} className="ide-c-muted" />
      </div>
      <div className="ide-tree">
        <TreeRows nodes={FILE_TREE} depth={0} />
      </div>
    </div>
  )
}

/* The center pane of the Files tool: the selected file, or the hint. */
export function FileTab() {
  const { selectedFile } = useIde()
  if (!selectedFile) {
    return <div className="ide-centerhint">Open a file from the Files panel.</div>
  }
  const lines = PACKAGE_JSON.split(`\n`)
  return (
    <div className="ide-code">
      <div className="ide-code-head">{selectedFile}</div>
      {lines.map((line, i) => (
        <div key={i} className="ide-code-line">
          <span className="ide-code-gutter">{i + 1}</span>
          <span className="ide-code-text">{tintJson(line)}</span>
        </div>
      ))}
    </div>
  )
}
