// #EXP-2027 (fixture `sidebar-compat.json`, its `$comment` is the spec): the
// per-kind sidebar layout (sections board, agent, app) kept in step with the
// mixed `team` list, so clients that still read and write the per-kind
// groups keep their folders and order. Server-only: the sidebarLayout router
// runs both directions in the save's transaction.
//
// `team` is the source of truth. `projectTeamToKind` derives a kind's rows
// from it (a team folder holding items of that kind = one folder row of the
// kind, linked through its `targetId`); `mergeKindIntoTeam` folds a
// per-kind save back into the team list.

import { SIDEBAR_FOLDER_DEFAULT_NAME, type SidebarEntry } from "./sidebar-folders"

export type SidebarCompatKind = `board` | `agent` | `app`
export type SidebarTargetKind = SidebarCompatKind | `session`

export const SIDEBAR_COMPAT_KINDS: readonly SidebarCompatKind[] = [`board`, `agent`, `app`]

/** One row to write in a section. `id` null = a new item row (upserted by
 *  (user, section, target)). A kind's folder row carries the team folder it
 *  mirrors in `targetId`. */
export interface SidebarCompatWrite {
  id: string | null
  kind: `folder` | `item`
  targetId: string | null
  parentId: string | null
  name: string | null
  sortOrder: number
}

export interface SidebarCompatWrites {
  upserts: SidebarCompatWrite[]
  /** Row ids to delete (folders and items). */
  deletes: string[]
}

interface Node {
  kind: `folder` | `item`
  /** A folder's entry id; an item's target id. */
  id: string
  name: string | null
  children: string[]
}

const byOrder = (a: SidebarEntry, b: SidebarEntry) =>
  a.sortOrder !== b.sortOrder ? a.sortOrder - b.sortOrder : a.id < b.id ? -1 : a.id > b.id ? 1 : 0

/** The placed nodes of a section as stored: folders and items with an entry,
 *  by sortOrder; an item whose parent names no folder sits at the top level.
 *  Empty folders are kept (a team folder being named has no item yet). */
function placed(entries: readonly SidebarEntry[]): Node[] {
  const sorted = [...entries].sort(byOrder)
  const folders = new Map<string, Node>()
  for (const entry of sorted) {
    if (entry.kind === `folder`) folders.set(entry.id, { kind: `folder`, id: entry.id, name: entry.name, children: [] })
  }
  const top: { order: SidebarEntry; node: Node }[] = []
  const seen = new Set<string>()
  for (const entry of sorted) {
    if (entry.kind === `folder`) {
      top.push({ order: entry, node: folders.get(entry.id)! })
      continue
    }
    if (entry.targetId === null || seen.has(entry.targetId)) continue
    seen.add(entry.targetId)
    const folder = entry.parentId !== null ? folders.get(entry.parentId) : undefined
    if (folder) folder.children.push(entry.targetId)
    else top.push({ order: entry, node: { kind: `item`, id: entry.targetId, name: null, children: [] } })
  }
  return top.map((t) => t.node)
}

/** A kind's rows as the team list shows them: its items in team order, a
 *  team folder holding any of them as ONE folder row of the kind (the
 *  existing row linked to it keeps its id, else `mint(teamFolderId)`), sort
 *  orders 1..n per container. The writes are what differs from `existing`;
 *  a row nothing places any more is deleted. Sessions never project. */
export function projectTeamToKind(input: {
  team: readonly SidebarEntry[]
  kindOf: (targetId: string) => SidebarTargetKind | null
  kind: SidebarCompatKind
  existing: readonly SidebarEntry[]
  mint: (teamFolderId: string) => string
}): SidebarCompatWrites {
  const { kind, kindOf } = input
  const linked = new Map<string, SidebarEntry>()
  const items = new Map<string, SidebarEntry>()
  for (const row of input.existing) {
    if (row.kind === `folder` && row.targetId !== null && !linked.has(row.targetId)) linked.set(row.targetId, row)
    if (row.kind === `item` && row.targetId !== null && !items.has(row.targetId)) items.set(row.targetId, row)
  }
  const desired: SidebarCompatWrite[] = []
  const itemRow = (targetId: string, parentId: string | null, sortOrder: number): SidebarCompatWrite => ({
    id: items.get(targetId)?.id ?? null,
    kind: `item`,
    targetId,
    parentId,
    name: null,
    sortOrder,
  })
  let index = 0
  for (const node of placed(input.team)) {
    if (node.kind === `item`) {
      if (kindOf(node.id) === kind) desired.push(itemRow(node.id, null, ++index))
      continue
    }
    const children = node.children.filter((id) => kindOf(id) === kind)
    if (children.length === 0) continue
    const folderId = linked.get(node.id)?.id ?? input.mint(node.id)
    desired.push({
      id: folderId,
      kind: `folder`,
      targetId: node.id,
      parentId: null,
      name: node.name ?? SIDEBAR_FOLDER_DEFAULT_NAME,
      sortOrder: ++index,
    })
    children.forEach((child, i) => desired.push(itemRow(child, folderId, i + 1)))
  }
  const byId = new Map(input.existing.map((row) => [row.id, row]))
  const upserts = desired.filter((row) => {
    const old = row.id !== null ? byId.get(row.id) : undefined
    return (
      !old ||
      old.kind !== row.kind ||
      old.targetId !== row.targetId ||
      old.parentId !== row.parentId ||
      old.name !== row.name ||
      old.sortOrder !== row.sortOrder
    )
  })
  const kept = new Set(desired.flatMap((row) => (row.id !== null ? [row.id] : [])))
  const deletes = input.existing.filter((row) => !kept.has(row.id)).map((row) => row.id)
  return { upserts, deletes }
}

/** A per-kind save folded back into the team list. The kind's items leave
 *  the team list and come back in the kind's new order: at the top level
 *  into the places its items or folders held (the rest after the last
 *  team node), inside a folder into the places its items held there (the
 *  rest at the folder's end). A kind folder maps to the team folder in its
 *  `targetId`; one with none (made by an older client) becomes a new team
 *  folder with the kind folder's name (`mint`), returned in `links`.
 *  Other kinds' and sessions' rows never move; a team folder the save left
 *  empty that held the kind's items is deleted. */
export function mergeKindIntoTeam(input: {
  team: readonly SidebarEntry[]
  kindRows: readonly SidebarEntry[]
  kind: SidebarCompatKind
  kindOf: (targetId: string) => SidebarTargetKind | null
  mint: (kindFolderId: string) => string
}): SidebarCompatWrites & { links: { kindFolderId: string; teamFolderId: string }[] } {
  const { kind, kindOf } = input
  const ofKind = (id: string) => kindOf(id) === kind
  const before = placed(input.team)
  const teamFolders = new Map(before.filter((n) => n.kind === `folder`).map((n) => [n.id, n]))
  const kindTree = placed(input.kindRows)
  const kindFolderRows = new Map(input.kindRows.filter((r) => r.kind === `folder`).map((r) => [r.id, r]))

  // Kind folder -> team folder.
  const links: { kindFolderId: string; teamFolderId: string }[] = []
  const teamFolderOf = new Map<string, string>()
  const created: Node[] = []
  for (const node of kindTree) {
    if (node.kind !== `folder`) continue
    const row = kindFolderRows.get(node.id)!
    const linkedId = row.targetId !== null && teamFolders.has(row.targetId) ? row.targetId : null
    if (linkedId) {
      teamFolderOf.set(node.id, linkedId)
      continue
    }
    const teamFolderId = input.mint(node.id)
    teamFolderOf.set(node.id, teamFolderId)
    links.push({ kindFolderId: node.id, teamFolderId })
    created.push({ kind: `folder`, id: teamFolderId, name: node.name, children: [] })
  }

  // Top level: the kind's slots, then refill them in the kind's order.
  type Slot = { node: Node } | { slot: true }
  const topSlots: Slot[] = []
  const hadKind = new Set<string>()
  for (const node of before) {
    if (node.kind === `item`) {
      topSlots.push(ofKind(node.id) ? { slot: true } : { node })
      continue
    }
    const kept = node.children.filter((id) => !ofKind(id))
    if (kept.length !== node.children.length) hadKind.add(node.id)
    // A folder keeps its place; its kind children are refilled below.
    topSlots.push({ node: { ...node, children: node.children } })
  }
  const linkedTeamFolders = new Set(teamFolderOf.values())
  const fillTop: Node[] = []
  for (const node of kindTree) {
    if (node.kind === `item`) fillTop.push({ kind: `item`, id: node.id, name: null, children: [] })
    else {
      const teamFolderId = teamFolderOf.get(node.id)!
      const fresh = created.find((n) => n.id === teamFolderId)
      if (fresh) fillTop.push(fresh)
    }
  }
  const top: Node[] = []
  for (const entry of topSlots) {
    if (`node` in entry) top.push(entry.node)
    else if (fillTop.length > 0) top.push(fillTop.shift()!)
  }
  top.push(...fillTop)

  // Inside each team folder: the kind's children in the kind's order.
  const kindChildren = new Map<string, string[]>()
  for (const node of kindTree) {
    if (node.kind === `folder`) kindChildren.set(teamFolderOf.get(node.id)!, node.children)
  }
  const result: Node[] = []
  for (const node of top) {
    if (node.kind === `item`) {
      result.push(node)
      continue
    }
    const incoming = [...(kindChildren.get(node.id) ?? [])]
    const children: string[] = []
    for (const child of node.children) {
      if (!ofKind(child)) children.push(child)
      else if (incoming.length > 0) children.push(incoming.shift()!)
    }
    children.push(...incoming)
    const emptied = children.length === 0 && hadKind.has(node.id) && !linkedTeamFolders.has(node.id)
    if (emptied) continue
    result.push({ ...node, children })
  }

  // The writes: every team node renumbered, only what differs.
  const byId = new Map(input.team.map((row) => [row.id, row]))
  const byTarget = new Map(input.team.filter((r) => r.kind === `item` && r.targetId !== null).map((r) => [r.targetId as string, r]))
  const upserts: SidebarCompatWrite[] = []
  const keep = new Set<string>()
  const writeItem = (targetId: string, parentId: string | null, sortOrder: number) => {
    const old = byTarget.get(targetId)
    if (old) keep.add(old.id)
    if (old && old.parentId === parentId && old.sortOrder === sortOrder) return
    upserts.push({ id: old?.id ?? null, kind: `item`, targetId, parentId, name: null, sortOrder })
  }
  result.forEach((node, index) => {
    if (node.kind === `item`) return writeItem(node.id, null, index + 1)
    keep.add(node.id)
    const old = byId.get(node.id)
    const name = node.name ?? SIDEBAR_FOLDER_DEFAULT_NAME
    if (!old || old.sortOrder !== index + 1 || old.name !== name || old.parentId !== null) {
      upserts.push({ id: node.id, kind: `folder`, targetId: null, parentId: null, name, sortOrder: index + 1 })
    }
    node.children.forEach((child, i) => writeItem(child, node.id, i + 1))
  })
  // A kind item the save took out of every place leaves the team list too;
  // other kinds' and sessions' rows are never deleted here.
  const deletes = input.team
    .filter((row) => !keep.has(row.id) && (row.kind === `folder` ? true : row.targetId !== null && ofKind(row.targetId)))
    .map((row) => row.id)
  return { upserts, deletes, links }
}
