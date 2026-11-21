import { and, eq, inArray, isNull } from "drizzle-orm"
import { sql } from "drizzle-orm"
import {
  SIDEBAR_COMPAT_KINDS,
  mergeKindIntoTeam,
  projectTeamToKind,
  type SidebarCompatKind,
  type SidebarCompatWrites,
  type SidebarTargetKind,
} from "@exp/domain-contract/sidebar-compat"
import type { SidebarEntry, SidebarSection } from "@exp/domain-contract/sidebar-folders"
import type { db } from "@/db/connection"
import { agents, apps, boards, sessions, sidebarLayout } from "@/db/schema"

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

// #EXP-2027 (fixture `sidebar-compat.json`): the per-kind sections board,
// agent and app stay the projection of the `team` list, so clients that
// still read and write them keep their folders and order. Runs in the
// save's transaction after the caller's own writes.

const isKind = (section: SidebarSection): section is SidebarCompatKind => section !== `team`

/** Each target's table (sessions: roots only); an id in none = null. */
async function targetKinds(tx: Tx, teamId: string, ids: readonly string[]): Promise<Map<string, SidebarTargetKind>> {
  const kinds = new Map<string, SidebarTargetKind>()
  const unique = [...new Set(ids)]
  if (unique.length === 0) return kinds
  const [boardRows, agentRows, appRows, sessionRows] = await Promise.all([
    tx.select({ id: boards.id }).from(boards).where(and(eq(boards.teamId, teamId), inArray(boards.id, unique))),
    tx.select({ id: agents.id }).from(agents).where(and(eq(agents.teamId, teamId), inArray(agents.id, unique))),
    tx.select({ id: apps.id }).from(apps).where(and(eq(apps.teamId, teamId), inArray(apps.id, unique))),
    tx
      .select({ id: sessions.id })
      .from(sessions)
      .where(and(eq(sessions.teamId, teamId), isNull(sessions.parentId), inArray(sessions.id, unique))),
  ])
  for (const row of boardRows) kinds.set(row.id, `board`)
  for (const row of agentRows) kinds.set(row.id, `agent`)
  for (const row of appRows) kinds.set(row.id, `app`)
  for (const row of sessionRows) kinds.set(row.id, `session`)
  return kinds
}

async function readRows(tx: Tx, userId: string, teamId: string): Promise<Map<SidebarSection, SidebarEntry[]>> {
  const rows = await tx
    .select({
      id: sidebarLayout.id,
      section: sidebarLayout.section,
      kind: sidebarLayout.kind,
      targetId: sidebarLayout.targetId,
      parentId: sidebarLayout.parentId,
      name: sidebarLayout.name,
      sortOrder: sidebarLayout.sortOrder,
    })
    .from(sidebarLayout)
    .where(and(eq(sidebarLayout.userId, userId), eq(sidebarLayout.teamId, teamId)))
  const bySection = new Map<SidebarSection, SidebarEntry[]>()
  for (const row of rows) {
    const section = row.section as SidebarSection
    const list = bySection.get(section) ?? []
    list.push({
      id: row.id,
      kind: row.kind as SidebarEntry[`kind`],
      targetId: row.targetId,
      parentId: row.parentId,
      name: row.name,
      sortOrder: row.sortOrder,
    })
    bySection.set(section, list)
  }
  return bySection
}

async function applyWrites(tx: Tx, userId: string, teamId: string, section: SidebarSection, writes: SidebarCompatWrites) {
  if (writes.deletes.length > 0) {
    await tx
      .delete(sidebarLayout)
      .where(and(eq(sidebarLayout.userId, userId), inArray(sidebarLayout.id, writes.deletes)))
  }
  for (const write of writes.upserts) {
    if (write.kind === `folder` && write.id) {
      await tx
        .insert(sidebarLayout)
        .values({
          id: write.id,
          userId,
          teamId,
          section,
          kind: `folder`,
          targetId: write.targetId,
          name: write.name,
          sortOrder: write.sortOrder,
        })
        .onConflictDoUpdate({
          target: sidebarLayout.id,
          set: { targetId: write.targetId, name: write.name, sortOrder: write.sortOrder, updatedAt: new Date() },
        })
    } else if (write.targetId) {
      // Raw SQL: the conflict target is a PARTIAL unique index.
      await tx.execute(sql`
        insert into sidebar_layout (user_id, team_id, section, kind, target_id, parent_id, sort_order)
        values (${userId}, ${teamId}, ${section}, 'item', ${write.targetId}, ${write.parentId}, ${write.sortOrder})
        on conflict (user_id, section, target_id) where target_id is not null
        do update set team_id = excluded.team_id, parent_id = excluded.parent_id, sort_order = excluded.sort_order, updated_at = now()
      `)
    }
  }
}

/** After a write into `section`: a per-kind write is folded into the team
 *  list first, then every kind is projected from the team list. */
export async function syncSidebarSections(tx: Tx, userId: string, teamId: string, section: SidebarSection): Promise<void> {
  let rows = await readRows(tx, userId, teamId)
  const ids = [...rows.values()].flat().flatMap((row) => (row.kind === `item` && row.targetId ? [row.targetId] : []))
  const kinds = await targetKinds(tx, teamId, ids)
  const kindOf = (id: string) => kinds.get(id) ?? null

  if (isKind(section)) {
    const merged = mergeKindIntoTeam({
      team: rows.get(`team`) ?? [],
      kindRows: rows.get(section) ?? [],
      kind: section,
      kindOf,
      mint: () => crypto.randomUUID(),
    })
    await applyWrites(tx, userId, teamId, `team`, merged)
    for (const link of merged.links) {
      await tx
        .update(sidebarLayout)
        .set({ targetId: link.teamFolderId, updatedAt: new Date() })
        .where(and(eq(sidebarLayout.id, link.kindFolderId), eq(sidebarLayout.userId, userId)))
    }
    rows = await readRows(tx, userId, teamId)
  }

  for (const kind of SIDEBAR_COMPAT_KINDS) {
    const writes = projectTeamToKind({
      team: rows.get(`team`) ?? [],
      kindOf,
      kind,
      existing: rows.get(kind) ?? [],
      mint: () => crypto.randomUUID(),
    })
    if (writes.upserts.length > 0 || writes.deletes.length > 0) await applyWrites(tx, userId, teamId, kind, writes)
  }
}

/** A per-kind folder's rename reaches the team folder it mirrors. */
export async function renameLinkedTeamFolder(tx: Tx, userId: string, kindFolderTargetId: string | null, name: string) {
  if (!kindFolderTargetId) return
  await tx
    .update(sidebarLayout)
    .set({ name, updatedAt: new Date() })
    .where(and(eq(sidebarLayout.id, kindFolderTargetId), eq(sidebarLayout.userId, userId), eq(sidebarLayout.section, `team`)))
}
