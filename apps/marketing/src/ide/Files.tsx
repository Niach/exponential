/* ─── Files (the footer Computer menu's Files, Screen::Files): the trunk
   file tree as a 320px pane inside the content card — the "Files" head
   with its refresh glyph, the branch pin, then 24px rows (chevron · folder
   or file glyph · text_sm name) — beside the viewer, which says "Open a
   file from the Files panel." until a file is picked. ─── */
import { Fragment } from "react"
import { FILE_TREE, PACKAGE_JSON, type FileNode } from "./data"
import { useIde } from "./state"
import { tintJson } from "./syntax"
import { IcBranch, IcChevDown, IcChevRight, IcFile, IcFiles, IcFolderOpen, IcRefresh } from "./icons"

const VIEWABLE = `package.json`

function TreeRows({ nodes, depth }: { nodes: FileNode[]; depth: number }) {
  const { expandedDirs, toggleDir, selectedFile, selectFile, interactive } = useIde()
  return (
    <>
      {nodes.map((n) => {
        const isDir = Boolean(n.children)
        const isExpanded = isDir && expandedDirs.has(n.path)
        const isSelected = !isDir && selectedFile === n.path
        /* The demo carries one file's text (the root package.json); the
           other files stay inert rather than show the wrong contents. */
        const openable = isDir || n.path === VIEWABLE
        const onClick =
          interactive && openable ? () => (isDir ? toggleDir(n.path) : selectFile(n.path)) : undefined
        return (
          <Fragment key={n.path}>
            <div
              className={`ide-tree-row${onClick ? ` is-click` : ``}${isSelected ? ` is-selected` : ``}`}
              style={{ paddingLeft: 8 + depth * 14 }}
              onClick={onClick}
            >
              {isDir ? (
                isExpanded ? (
                  <IcChevDown size={10.5} className="ide-c-muted" />
                ) : (
                  <IcChevRight size={10.5} className="ide-c-muted" />
                )
              ) : (
                <span className="ide-tree-spacer" />
              )}
              {isDir ? (
                isExpanded ? (
                  <IcFolderOpen size={12.25} className="ide-c-muted" />
                ) : (
                  <IcFiles size={12.25} className="ide-c-muted" />
                )
              ) : (
                <IcFile size={12.25} className="ide-c-muted" />
              )}
              <span className="ide-tree-name">{n.name}</span>
            </div>
            {isDir && isExpanded && n.children && <TreeRows nodes={n.children} depth={depth + 1} />}
          </Fragment>
        )
      })}
    </>
  )
}

export function FilesPanel() {
  return (
    <div className="ide-pane">
      <div className="ide-pane-head">
        <IcFiles size={12.25} className="ide-c-muted" />
        <span className="ide-pane-title">Files</span>
        <span className="ide-flex1" />
        <span className="ide-icon-btn" title="Refresh">
          <IcRefresh size={12.25} />
        </span>
      </div>
      <div className="ide-branchrow">
        <IcBranch size={10.5} className="ide-c-muted" />
        <span>master</span>
        <IcChevDown size={10.5} className="ide-c-muted" />
      </div>
      <div className="ide-tree">
        <TreeRows nodes={FILE_TREE} depth={0} />
      </div>
    </div>
  )
}

/* The viewer: the selected file, or the hint. */
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
