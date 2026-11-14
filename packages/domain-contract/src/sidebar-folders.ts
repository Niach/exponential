// #EXP-1832: sidebar folders + drag reorder, the pure reference of
// `fixtures/sidebar-folders.json` (its `$comment` is the spec). Web, desktop
// (Rust `domain::sidebar_folders`), iOS (ExpCore) and Android replay it.
//
// One synced `sidebar_layout` row per (member, team, section) node: a FOLDER
// (`kind: folder`, a name, never navigable) or an ITEM entry placing one board,
// agent or app (`targetId`), optionally inside a folder (`parentId`). Two
// layers max: a folder holds items only. A row with no entry keeps the
// default order (the caller's `items`), after the placed ones.

export type SidebarSection = `board` | `agent` | `app`
export type SidebarEntryKind = `folder` | `item`

export const SIDEBAR_SECTIONS: readonly SidebarSection[] = [`board`, `agent`, `app`]
export const SIDEBAR_FOLDER_DEFAULT_NAME = `New folder`
/** Nesting levels: top level + one folder level. */
export const SIDEBAR_MAX_DEPTH = 2

/** One synced `sidebar_layout` row of a section (the shape's columns). */
export interface SidebarEntry {
  id: string
  kind: SidebarEntryKind
  /** The board / agent / app an item places; null on a folder. */
  targetId: string | null
  /** The folder an item sits in; null = top level (always null on a folder). */
  parentId: string | null
  /** A folder's name; null on an item. */
  name: string | null
  sortOrder: number
}

export interface SidebarItemNode {
  kind: `item`
  /** The target id (board / agent / app). */
  id: string
}

export interface SidebarFolderNode {
  kind: `folder`
  /** The folder entry's id. */
  id: string
  name: string
  /** Target ids, in order; never empty. */
  children: string[]
}

export type SidebarNode = SidebarItemNode | SidebarFolderNode

/** A List `move` event: `id` lands before / after / into the named node
 *  (exactly one of the three). Ids are node ids: a target id for an item, the
 *  entry id for a folder. */
export interface SidebarMove {
  id: string
  before?: string
  after?: string
  into?: string
}

/** One row to write. `id` null = a new ITEM entry (the server upserts it by
 *  (user, section, target)); a new folder carries the caller's `newFolderId`. */
export interface SidebarEntryWrite {
  id: string | null
  kind: SidebarEntryKind
  targetId: string | null
  parentId: string | null
  name: string | null
  sortOrder: number
}

export interface SidebarWrites {
  upserts: SidebarEntryWrite[]
  /** Entry ids to delete (folders that became empty / were ungrouped). */
  deletes: string[]
  /** The folder this move created: the client edits its name in place. */
  createdFolderId: string | null
}

export interface SidebarMoveInput {
  /** The section's visible target ids in DEFAULT order (boards and agents
   *  by sortOrder, apps by name). */
  items: readonly string[]
  entries: readonly SidebarEntry[]
  move: SidebarMove
  /** The id a folder created by this move takes (a client-minted uuid). */
  newFolderId: string
}

const compareEntries = (a: SidebarEntry, b: SidebarEntry) =>
  a.sortOrder !== b.sortOrder ? a.sortOrder - b.sortOrder : a.id < b.id ? -1 : a.id > b.id ? 1 : 0

/** The section as the sidebar shows it. Placed nodes (folders and items with
 *  an entry) first by `sortOrder` (ties by entry id), then every item with no
 *  entry in default order. An entry whose target is not in `items` is
 *  skipped; an item whose `parentId` names no folder sits at the top level; a
 *  folder's `parentId` is ignored (two layers max); a folder with no visible
 *  item is not shown. */
export function sidebarTree(items: readonly string[], entries: readonly SidebarEntry[]): SidebarNode[] {
  const visible = new Set(items)
  const folders = new Map<string, SidebarEntry>()
  const itemEntries = new Map<string, SidebarEntry>()
  for (const entry of [...entries].sort(compareEntries)) {
    if (entry.kind === `folder`) folders.set(entry.id, entry)
    else if (entry.targetId !== null && visible.has(entry.targetId) && !itemEntries.has(entry.targetId)) {
      itemEntries.set(entry.targetId, entry)
    }
  }
  const children = new Map<string, SidebarEntry[]>()
  const top: SidebarEntry[] = []
  for (const entry of itemEntries.values()) {
    if (entry.parentId !== null && folders.has(entry.parentId)) {
      const list = children.get(entry.parentId) ?? []
      list.push(entry)
      children.set(entry.parentId, list)
    } else top.push(entry)
  }
  for (const folder of folders.values()) if (children.has(folder.id)) top.push(folder)
  top.sort(compareEntries)
  const nodes: SidebarNode[] = top.map((entry) =>
    entry.kind === `folder`
      ? {
          kind: `folder`,
          id: entry.id,
          name: entry.name ?? SIDEBAR_FOLDER_DEFAULT_NAME,
          children: (children.get(entry.id) ?? []).sort(compareEntries).map((child) => child.targetId as string),
        }
      : { kind: `item`, id: entry.targetId as string }
  )
  for (const item of items) if (!itemEntries.has(item)) nodes.push({ kind: `item`, id: item })
  return nodes
}

interface Located {
  /** The folder holding it; null = top level. */
  folder: SidebarFolderNode | null
  node: SidebarNode
}

function locate(tree: SidebarNode[], id: string): Located | null {
  for (const node of tree) {
    if (node.id === id) return { folder: null, node }
    if (node.kind === `folder`) {
      for (const child of node.children) if (child === id) return { folder: node, node: { kind: `item`, id: child } }
    }
  }
  return null
}

/** Writes the tree as rows: every node's `sortOrder` = its 1-based index in
 *  its container; only rows that differ from their entry (or have none). */
function materialize(
  tree: SidebarNode[],
  entries: readonly SidebarEntry[],
  deletes: string[],
  createdFolderId: string | null
): SidebarWrites {
  const byId = new Map(entries.map((entry) => [entry.id, entry]))
  const byTarget = new Map<string, SidebarEntry>()
  for (const entry of [...entries].sort(compareEntries)) {
    if (entry.kind === `item` && entry.targetId !== null && !byTarget.has(entry.targetId)) byTarget.set(entry.targetId, entry)
  }
  const upserts: SidebarEntryWrite[] = []
  const writeItem = (targetId: string, parentId: string | null, sortOrder: number) => {
    const entry = byTarget.get(targetId)
    if (entry && entry.parentId === parentId && entry.sortOrder === sortOrder) return
    upserts.push({ id: entry?.id ?? null, kind: `item`, targetId, parentId, name: null, sortOrder })
  }
  tree.forEach((node, index) => {
    if (node.kind === `item`) return writeItem(node.id, null, index + 1)
    const entry = byId.get(node.id)
    if (!entry || entry.sortOrder !== index + 1 || entry.name !== node.name || entry.parentId !== null) {
      upserts.push({ id: node.id, kind: `folder`, targetId: null, parentId: null, name: node.name, sortOrder: index + 1 })
    }
    node.children.forEach((child, childIndex) => writeItem(child, node.id, childIndex + 1))
  })
  return { upserts, deletes, createdFolderId }
}

/** Applies a List `move` to the section. Null = not allowed (an unknown id,
 *  a node onto itself, a folder into anything or beside a folder's item, an
 *  item into an item that sits in a folder). `before`/`after` = beside the
 *  target in its container; `into` a folder = appended to it; `into` an item
 *  = a NEW folder (`newFolderId`, the default name) at the item's place
 *  holding the item then the dragged one. A folder the move empties is
 *  deleted. */
export function sidebarMove(input: SidebarMoveInput): SidebarWrites | null {
  const { move } = input
  const positions = ([`before`, `after`, `into`] as const).filter((key) => move[key] !== undefined)
  if (positions.length !== 1) return null
  const position = positions[0]
  const targetId = move[position] as string
  if (targetId === move.id) return null
  const tree = sidebarTree(input.items, input.entries)
  const dragged = locate(tree, move.id)
  const target = locate(tree, targetId)
  if (!dragged || !target) return null
  if (dragged.node.kind === `folder` && (position === `into` || target.folder !== null)) return null
  if (position === `into` && target.node.kind === `item` && target.folder !== null) return null

  // Take the dragged node out.
  const source = dragged.folder
  if (source) source.children.splice(source.children.indexOf(move.id), 1)
  else tree.splice(tree.findIndex((node) => node.id === move.id), 1)

  let createdFolderId: string | null = null
  if (position === `into`) {
    if (target.node.kind === `folder`) {
      ;(tree.find((node) => node.id === targetId) as SidebarFolderNode).children.push(move.id)
    } else {
      createdFolderId = input.newFolderId
      const index = tree.findIndex((node) => node.id === targetId)
      tree.splice(index, 1, {
        kind: `folder`,
        id: input.newFolderId,
        name: SIDEBAR_FOLDER_DEFAULT_NAME,
        children: [targetId, move.id],
      })
    }
  } else {
    const draggedNode: SidebarNode = dragged.node.kind === `folder` ? dragged.node : { kind: `item`, id: move.id }
    if (target.folder) {
      const list = target.folder.children
      list.splice(list.indexOf(targetId) + (position === `after` ? 1 : 0), 0, move.id)
    } else {
      const index = tree.findIndex((node) => node.id === targetId)
      tree.splice(index + (position === `after` ? 1 : 0), 0, draggedNode)
    }
  }

  const deletes: string[] = []
  if (source && source.children.length === 0) {
    tree.splice(tree.indexOf(source), 1)
    deletes.push(source.id)
  }
  return materialize(tree, input.entries, deletes, createdFolderId)
}

/** Ungroup: the folder's items take its place at the top level, in order,
 *  and the folder is deleted. Null = no such (visible) folder. */
export function sidebarUngroup(
  items: readonly string[],
  entries: readonly SidebarEntry[],
  folderId: string
): SidebarWrites | null {
  const tree = sidebarTree(items, entries)
  const index = tree.findIndex((node) => node.kind === `folder` && node.id === folderId)
  if (index < 0) return null
  const folder = tree[index] as SidebarFolderNode
  tree.splice(index, 1, ...folder.children.map((id): SidebarNode => ({ kind: `item`, id })))
  return materialize(tree, entries, [folderId], null)
}

/** A folder name as saved: trimmed, at most 80 characters; empty = the
 *  default (Esc / an emptied field keeps "New folder"). */
export function sidebarFolderName(input: string): string {
  const name = input.trim().slice(0, SIDEBAR_FOLDER_NAME_MAX)
  return name === `` ? SIDEBAR_FOLDER_DEFAULT_NAME : name
}
export const SIDEBAR_FOLDER_NAME_MAX = 80

/** The device-local expanded flag's key (localStorage, the IDE settings
 *  file, UserDefaults, DataStore); a folder with no stored flag is OPEN. */
export function sidebarFolderOpenKey(teamId: string, folderId: string): string {
  return `sidebar-folder-open:${teamId}:${folderId}`
}

/** One List row: what a client's List template gets. A folder row carries
 *  `group: true` and its item count; a closed folder's items are left out. */
export interface SidebarRow {
  kind: SidebarEntryKind
  /** Node id (target id for an item, entry id for a folder). */
  id: string
  depth: 0 | 1
  group: boolean
  /** Folders only. */
  name: string | null
  open: boolean
  count: number
}

export function sidebarRows(tree: readonly SidebarNode[], isOpen: (folderId: string) => boolean): SidebarRow[] {
  const rows: SidebarRow[] = []
  for (const node of tree) {
    if (node.kind === `item`) {
      rows.push({ kind: `item`, id: node.id, depth: 0, group: false, name: null, open: false, count: 0 })
      continue
    }
    const open = isOpen(node.id)
    rows.push({ kind: `folder`, id: node.id, depth: 0, group: true, name: node.name, open, count: node.children.length })
    if (open) {
      for (const child of node.children) {
        rows.push({ kind: `item`, id: child, depth: 1, group: false, name: null, open: false, count: 0 })
      }
    }
  }
  return rows
}
