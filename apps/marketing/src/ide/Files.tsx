/* ─── File tree sidebar panel + read-only code tab (package.json) ─── */
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
  IcRefresh,
} from "./icons"

function TreeRows({ nodes, depth }: { nodes: FileNode[]; depth: number }) {
  const { expandedDirs, toggleDir, selectedFile, selectFile, openFile, interactive, active } =
    useIde()
  return (
    <>
      {nodes.map((n) => {
        const isDir = Boolean(n.children)
        const isExpanded = isDir && expandedDirs.has(n.path)
        const isOpenFile = !isDir && n.path === `package.json` && active === `file:package.json`
        const isSelected = !isDir && selectedFile === n.path && !isOpenFile
        const onClick = interactive
          ? () => {
              if (isDir) {
                toggleDir(n.path)
              } else {
                selectFile(n.path)
                if (n.path === `package.json`) openFile(n.path)
              }
            }
          : undefined
        return (
          <Fragment key={n.path}>
            <div
              className={`ide-tree-row${interactive ? ` is-click` : ``}${isOpenFile ? ` is-openfile` : ``}${isSelected ? ` is-selected` : ``}${n.dim ? ` is-dim` : ``}`}
              style={{ paddingLeft: 8 + depth * 14 }}
              onClick={onClick}
            >
              {isDir ? (
                isExpanded ? (
                  <IcChevDown size={14} className="ide-c-muted" />
                ) : (
                  <IcChevRight size={14} className="ide-c-muted" />
                )
              ) : (
                <span className="ide-tree-spacer" />
              )}
              {isDir ? (
                isExpanded ? (
                  <IcFolderOpen size={14} className="ide-c-muted" />
                ) : (
                  <IcFolder size={14} className="ide-c-muted" />
                )
              ) : (
                <IcFile size={14} className="ide-c-muted" />
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
        icon={<IcFolder size={14} className="ide-c-muted" />}
        title="Files"
        trailing={
          <button className="ide-ghost ide-icbtn" type="button" title="Refresh">
            <IcRefresh size={12} />
          </button>
        }
      />
      <div className="ide-tree">
        <TreeRows nodes={FILE_TREE} depth={0} />
      </div>
    </div>
  )
}

export function FileTab() {
  const lines = PACKAGE_JSON.split(`\n`)
  return (
    <div className="ide-code">
      {lines.map((line, i) => (
        <div key={i} className="ide-code-line">
          <span className="ide-code-gutter">{i + 1}</span>
          <span className="ide-code-text">{tintJson(line)}</span>
        </div>
      ))}
    </div>
  )
}
