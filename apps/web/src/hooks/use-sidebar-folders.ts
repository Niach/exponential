import { useCallback, useMemo, useState, useSyncExternalStore } from "react"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { toast } from "@exp/ui"
import {
  SIDEBAR_FOLDER_DEFAULT_NAME,
  sidebarFolderOpenKey,
  sidebarMove,
  sidebarMoveToNewFolder,
  sidebarRows,
  sidebarTree,
  sidebarUngroup,
  type SidebarEntry,
  type SidebarMove,
  type SidebarRow,
  type SidebarSection,
  type SidebarWrites,
} from "@exp/domain-contract/sidebar-folders"
import { sidebarDropSession, type SidebarSessionInput } from "@exp/domain-contract/sidebar-tree"
import { sidebarLayoutCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { teamListMove } from "@/hooks/use-sidebar-tree"

// #EXP-1832 (fixtures/sidebar-folders.json): one sidebar group's folders and
// order, x web. #EXP-2027 (sidebar-tree.json): section `team` = ONE mixed
// list per team (its boards, agents, apps and placed sessions); a move onto
// the team row lands at the list's end, New folder appends an empty folder
// SHOWN while it is named, and a root session dropped from the Active band
// joins the list (`sidebarDropSession`). The member's synced `sidebar_layout` rows of the active team
// and section feed the pure `sidebarTree` / `sidebarRows`; a drop runs
// `sidebarMove` locally, shows its result AT ONCE (an optimistic overlay
// until the write's txId lands) and sends the writes (`sidebarLayout.save`).
// The expanded flag is device-local (localStorage, `sidebarFolderOpenKey`,
// default open).

const OPEN_EVENT = `exp:sidebar-folder-open`

function storage(): Storage | null {
  try {
    return typeof window === `undefined` ? null : window.localStorage
  } catch {
    return null
  }
}

function subscribeOpen(callback: () => void) {
  if (typeof window === `undefined`) return () => {}
  window.addEventListener(OPEN_EVENT, callback)
  window.addEventListener(`storage`, callback)
  return () => {
    window.removeEventListener(OPEN_EVENT, callback)
    window.removeEventListener(`storage`, callback)
  }
}

/** Every closed folder's key, as one string (a stable snapshot). */
function closedSnapshot(): string {
  const store = storage()
  if (!store) return ``
  const keys: string[] = []
  for (let i = 0; i < store.length; i++) {
    const key = store.key(i)
    if (key?.startsWith(`sidebar-folder-open:`) && store.getItem(key) === `0`) keys.push(key)
  }
  return keys.sort().join(`\n`)
}

export function setSidebarFolderOpen(teamId: string, folderId: string, open: boolean) {
  const store = storage()
  if (!store) return
  const key = sidebarFolderOpenKey(teamId, folderId)
  if (open) store.removeItem(key)
  else store.setItem(key, `0`)
  window.dispatchEvent(new Event(OPEN_EVENT))
}

const toEntry = (row: {
  id: string
  kind: string
  targetId: string | null
  parentId: string | null
  name: string | null
  sortOrder: number
}): SidebarEntry => ({
  id: row.id,
  kind: row.kind === `folder` ? `folder` : `item`,
  targetId: row.targetId,
  parentId: row.parentId,
  name: row.name,
  sortOrder: row.sortOrder,
})

/** The entries after `writes`, for the optimistic overlay. */
export function applySidebarWrites(entries: readonly SidebarEntry[], writes: SidebarWrites): SidebarEntry[] {
  const deleted = new Set(writes.deletes)
  const out = new Map(entries.filter((entry) => !deleted.has(entry.id)).map((entry) => [entry.id, entry]))
  for (const write of writes.upserts) {
    const existing =
      write.id !== null
        ? out.get(write.id)
        : [...out.values()].find((entry) => entry.kind === `item` && entry.targetId === write.targetId)
    const id = existing?.id ?? write.id ?? `pending:${write.targetId}`
    out.set(id, { id, kind: write.kind, targetId: write.targetId, parentId: write.parentId, name: write.name, sortOrder: write.sortOrder })
  }
  return [...out.values()]
}

export interface SidebarFolders {
  rows: SidebarRow[]
  /** The list's visible targets in DEFAULT order. */
  items: readonly string[]
  /** A folder's items, open or folded. */
  itemsOf: (folderId: string) => readonly string[]
  /** The folder whose name is being edited in place. */
  editingId: string | null
  setEditingId: (id: string | null) => void
  move: (move: SidebarMove) => void
  toggle: (folderId: string) => void
  rename: (folderId: string, name: string) => void
  ungroup: (folderId: string) => void
  /** New folder: an empty `New folder` at the end, its name in edit. */
  addFolder: () => void
  /** #EXP-2124 Move to folder › New folder: a new folder holding the item,
   *  its name in edit. */
  moveToNewFolder: (id: string) => void
  /** A root session dropped onto the list (refused for another team / a child). */
  dropSession: (session: SidebarSessionInput, move: SidebarMove) => void
}

/** The empty folder being named is SHOWN (sidebar-tree.json New folder):
 *  appended at the end while `editingId` names a folder entry with no row. */
export function withEditingFolder(
  rows: readonly SidebarRow[],
  entries: readonly SidebarEntry[],
  editingId: string | null
): SidebarRow[] {
  if (!editingId || rows.some((row) => row.id === editingId)) return [...rows]
  const folder = entries.find((entry) => entry.id === editingId && entry.kind === `folder`)
  if (!folder) return [...rows]
  return [
    ...rows,
    { kind: `folder`, id: folder.id, depth: 0, group: true, name: folder.name ?? SIDEBAR_FOLDER_DEFAULT_NAME, open: true, count: 0 },
  ]
}

/** `ids` = the section's visible targets in DEFAULT order, or (#EXP-2027,
 *  a team list whose placed sessions are entries) a function of the entries. */
export function useSidebarFolders(
  teamId: string | undefined,
  section: SidebarSection,
  ids: readonly string[] | ((entries: readonly SidebarEntry[]) => readonly string[])
): SidebarFolders {
  const { data } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ s: sidebarLayoutCollection })
            .where(({ s }) => and(eq(s.teamId, teamId), eq(s.section, section)))
        : undefined,
    [teamId, section]
  )
  const synced = useMemo(() => (data ?? []).map(toEntry), [data])
  const [optimistic, setOptimistic] = useState<SidebarEntry[] | null>(null)
  const entries = optimistic ?? synced
  const [editingId, setEditingId] = useState<string | null>(null)
  const closed = useSyncExternalStore(subscribeOpen, closedSnapshot, () => ``)
  const closedSet = useMemo(() => new Set(closed.split(`\n`)), [closed])
  const resolved = typeof ids === `function` ? ids(entries) : ids
  const idsKey = resolved.join(`,`)
  // eslint-disable-next-line react-hooks/exhaustive-deps -- keyed by content
  const stableIds = useMemo(() => [...resolved], [idsKey])

  const rows = useMemo(() => {
    const tree = sidebarTree(stableIds, entries)
    const shown = sidebarRows(tree, (folderId) => !teamId || !closedSet.has(sidebarFolderOpenKey(teamId, folderId)))
    return withEditingFolder(shown, entries, editingId)
  }, [stableIds, entries, closedSet, teamId, editingId])

  const itemsOf = useCallback(
    (folderId: string) =>
      sidebarRows(sidebarTree(stableIds, entries), () => true).reduce<{ folder: string | null; ids: string[] }>(
        (at, row) =>
          row.depth === 0
            ? { ...at, folder: row.id }
            : at.folder === folderId
              ? { ...at, ids: [...at.ids, row.id] }
              : at,
        { folder: null, ids: [] }
      ).ids,
    [stableIds, entries]
  )

  const send = useCallback(
    (writes: SidebarWrites | null) => {
      if (!teamId || !writes) return
      if (writes.upserts.length === 0 && writes.deletes.length === 0) return
      setOptimistic(applySidebarWrites(entries, writes))
      trpc.sidebarLayout.save
        .mutate({ teamId, section, upserts: writes.upserts, deletes: writes.deletes })
        .then(({ txId }) => sidebarLayoutCollection.utils.awaitTxId(txId))
        .catch(() => toast.error(`Could not save the sidebar`))
        .finally(() => setOptimistic(null))
    },
    [teamId, section, entries]
  )

  /** A team list's move onto its team row → the list's end. */
  const resolveMove = useCallback(
    (moveInput: SidebarMove) =>
      section === `team` && teamId ? teamListMove(moveInput, teamId, stableIds, entries) : { move: moveInput },
    [section, teamId, stableIds, entries]
  )

  const move = useCallback(
    (moveInput: SidebarMove) => {
      const resolvedMove = resolveMove(moveInput)
      if (!resolvedMove || !(`move` in resolvedMove)) return
      const writes = sidebarMove({ items: stableIds, entries, move: resolvedMove.move, newFolderId: crypto.randomUUID() })
      send(writes)
      if (writes?.createdFolderId) setEditingId(writes.createdFolderId)
    },
    [resolveMove, stableIds, entries, send]
  )

  const dropSession = useCallback(
    (session: SidebarSessionInput, moveInput: SidebarMove) => {
      if (!teamId) return
      const resolvedMove = resolveMove(moveInput)
      if (!resolvedMove) return
      if (!(`move` in resolvedMove)) {
        // An empty team: the session is its list's first node.
        if (session.teamId !== teamId || session.parentId !== null) return
        send({
          upserts: [{ id: null, kind: `item`, targetId: session.id, parentId: null, name: null, sortOrder: 1 }],
          deletes: [],
          createdFolderId: null,
        })
        return
      }
      const writes = sidebarDropSession({
        teamId,
        items: stableIds,
        entries,
        session,
        move: resolvedMove.move,
        newFolderId: crypto.randomUUID(),
      })
      send(writes)
      if (writes?.createdFolderId) setEditingId(writes.createdFolderId)
    },
    [teamId, resolveMove, stableIds, entries, send]
  )

  const addFolder = useCallback(() => {
    if (!teamId) return
    const id = crypto.randomUUID()
    const top = sidebarTree(stableIds, entries).length
    send({
      upserts: [{ id, kind: `folder`, targetId: null, parentId: null, name: SIDEBAR_FOLDER_DEFAULT_NAME, sortOrder: top + 1 }],
      deletes: [],
      createdFolderId: id,
    })
    setEditingId(id)
  }, [teamId, stableIds, entries, send])

  const moveToNewFolder = useCallback(
    (id: string) => {
      const writes = sidebarMoveToNewFolder(stableIds, entries, id, crypto.randomUUID())
      send(writes)
      if (writes?.createdFolderId) setEditingId(writes.createdFolderId)
    },
    [stableIds, entries, send]
  )

  const ungroup = useCallback(
    (folderId: string) => send(sidebarUngroup(stableIds, entries, folderId)),
    [stableIds, entries, send]
  )

  const rename = useCallback(
    (folderId: string, name: string) => {
      setEditingId(null)
      trpc.sidebarLayout.rename
        .mutate({ id: folderId, name })
        .then(({ txId }) => sidebarLayoutCollection.utils.awaitTxId(txId))
        .catch(() => toast.error(`Could not rename the folder`))
    },
    []
  )

  const toggle = useCallback(
    (folderId: string) => {
      if (!teamId) return
      setSidebarFolderOpen(teamId, folderId, closedSet.has(sidebarFolderOpenKey(teamId, folderId)))
    },
    [teamId, closedSet]
  )

  return { rows, items: stableIds, itemsOf, editingId, setEditingId, move, toggle, rename, ungroup, addFolder, moveToNewFolder, dropSession }
}
