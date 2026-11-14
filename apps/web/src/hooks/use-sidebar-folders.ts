import { useCallback, useMemo, useState, useSyncExternalStore } from "react"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { toast } from "@exp/ui"
import {
  sidebarFolderOpenKey,
  sidebarMove,
  sidebarRows,
  sidebarTree,
  sidebarUngroup,
  type SidebarEntry,
  type SidebarMove,
  type SidebarRow,
  type SidebarSection,
  type SidebarWrites,
} from "@exp/domain-contract/sidebar-folders"
import { sidebarLayoutCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// #EXP-1832 (fixtures/sidebar-folders.json): one sidebar group's folders and
// order, x web. The member's synced `sidebar_layout` rows of the active team
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
  /** The folder whose name is being edited in place. */
  editingId: string | null
  setEditingId: (id: string | null) => void
  move: (move: SidebarMove) => void
  toggle: (folderId: string) => void
  rename: (folderId: string, name: string) => void
  ungroup: (folderId: string) => void
}

/** `ids` = the section's visible targets in DEFAULT order. */
export function useSidebarFolders(
  teamId: string | undefined,
  section: SidebarSection,
  ids: readonly string[]
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
  const idsKey = ids.join(`,`)
  // eslint-disable-next-line react-hooks/exhaustive-deps -- keyed by content
  const stableIds = useMemo(() => [...ids], [idsKey])

  const rows = useMemo(() => {
    const tree = sidebarTree(stableIds, entries)
    return sidebarRows(tree, (folderId) => !teamId || !closedSet.has(sidebarFolderOpenKey(teamId, folderId)))
  }, [stableIds, entries, closedSet, teamId])

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

  const move = useCallback(
    (moveInput: SidebarMove) => {
      const writes = sidebarMove({ items: stableIds, entries, move: moveInput, newFolderId: crypto.randomUUID() })
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

  return { rows, editingId, setEditingId, move, toggle, rename, ungroup }
}
