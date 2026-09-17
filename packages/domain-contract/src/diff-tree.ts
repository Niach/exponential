// EXP-916: the file TREE a Changes surface lists beside its file cards — the
// sidebar on web (≥md) and the desktop, the file sheet on a phone. One pure
// builder over the shared `DiffFile` model, hand-mirrored ×4 (web `@exp/ui`
// `FileDiffTree`, desktop `domain::diff_tree`, iOS
// `ExpCore/Sources/Domain/DiffTree.swift`, Android `domain/DiffTree.kt`) and
// byte-locked by `fixtures/diff/tree.json`.
//
// Rules:
// - a path splits on `/`; every segment but the last is a directory;
// - per level, directories come before files, each group sorted by the
//   lower-cased name compared by UTF-16 code unit (never a locale compare —
//   it is not portable), ties broken by the raw name the same way;
// - a directory with exactly ONE child, a directory, and no files of its own
//   compacts into that child — `apps/web/src` is one node (VS Code's compact
//   folders), and the compaction repeats down the chain;
// - a directory's `additions`/`deletions`/`files` are its subtree sums;
// - a non-blank `query` returns a FLAT list of the FILE nodes whose path
//   contains it, case-insensitively, in INPUT order — the filter is a search
//   result, not a pruned tree.

import type { DiffFile } from "./diff"

export interface DiffTreeNode {
  kind: `dir` | `file`
  /** The full path from the root (`apps/web/src` for a compacted dir). */
  path: string
  /** The label: the last segment, or the compacted chain `a/b/c`. */
  name: string
  additions: number
  deletions: number
  /** Files in the subtree (1 for a file). */
  files: number
  /** The index into the input `files`; -1 for a directory. */
  index: number
  children: DiffTreeNode[]
}

interface Building {
  node: DiffTreeNode
  dirs: Map<string, Building>
  leaves: DiffTreeNode[]
}

function newDir(path: string, name: string): Building {
  return {
    node: {
      kind: `dir`,
      path,
      name,
      additions: 0,
      deletions: 0,
      files: 0,
      index: -1,
      children: [],
    },
    dirs: new Map(),
    leaves: [],
  }
}

/** Lower-cased code-unit order, then the raw name — portable everywhere. */
function byName(a: DiffTreeNode, b: DiffTreeNode): number {
  const la = a.name.toLowerCase()
  const lb = b.name.toLowerCase()
  if (la < lb) return -1
  if (la > lb) return 1
  if (a.name < b.name) return -1
  if (a.name > b.name) return 1
  return 0
}

function finish(dir: Building): DiffTreeNode {
  let dirs = [...dir.dirs.values()]
  // Compact a lone child directory into this one, down the chain.
  let node = dir.node
  let leaves = dir.leaves
  while (dirs.length === 1 && leaves.length === 0 && node.path !== ``) {
    const only = dirs[0]
    node = {
      ...node,
      path: only.node.path,
      name: `${node.name}/${only.node.name}`,
    }
    dirs = [...only.dirs.values()]
    leaves = only.leaves
  }
  const children = [
    ...dirs.map(finish).sort(byName),
    ...leaves.slice().sort(byName),
  ]
  let additions = 0
  let deletions = 0
  let files = 0
  for (const child of children) {
    additions += child.additions
    deletions += child.deletions
    files += child.files
  }
  return { ...node, additions, deletions, files, children }
}

export function diffFileTree(
  files: readonly DiffFile[],
  query = ``
): DiffTreeNode[] {
  const needle = query.trim().toLowerCase()
  if (needle) {
    const out: DiffTreeNode[] = []
    files.forEach((file, index) => {
      if (!file.path.toLowerCase().includes(needle)) return
      out.push(leaf(file, index, file.path))
    })
    return out
  }
  const root = newDir(``, ``)
  files.forEach((file, index) => {
    const segments = file.path.split(`/`)
    let at = root
    for (let i = 0; i < segments.length - 1; i++) {
      const name = segments[i]
      const path = segments.slice(0, i + 1).join(`/`)
      let next = at.dirs.get(name)
      if (!next) {
        next = newDir(path, name)
        at.dirs.set(name, next)
      }
      at = next
    }
    at.leaves.push(leaf(file, index, segments[segments.length - 1]))
  })
  return finish(root).children
}

function leaf(file: DiffFile, index: number, name: string): DiffTreeNode {
  return {
    kind: `file`,
    path: file.path,
    name,
    additions: file.additions,
    deletions: file.deletions,
    files: 1,
    index,
    children: [],
  }
}

/**
 * The byte-lock projection: one line per node, two spaces per depth, a
 * directory as `name +a -d (files)` and a file as `name +a -d`. ASCII `-`,
 * like `renderDiff`.
 */
export function renderDiffTree(nodes: readonly DiffTreeNode[]): string[] {
  const out: string[] = []
  const walk = (list: readonly DiffTreeNode[], depth: number) => {
    for (const node of list) {
      const indent = `  `.repeat(depth)
      const counts = `+${node.additions} -${node.deletions}`
      out.push(
        node.kind === `dir`
          ? `${indent}${node.name} ${counts} (${node.files})`
          : `${indent}${node.name} ${counts}`
      )
      walk(node.children, depth + 1)
    }
  }
  walk(nodes, 0)
  return out
}
