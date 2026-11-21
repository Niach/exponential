// @vitest-environment node
import { readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"
import { sql } from "drizzle-orm"
import { projectTeamToKind, type SidebarCompatKind, type SidebarTargetKind } from "@exp/domain-contract/sidebar-compat"
import { sidebarTree, type SidebarEntry } from "@exp/domain-contract/sidebar-folders"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"
import { syncSidebarSections } from "@/lib/sidebar-compat-sync"

// #EXP-2027 (sidebar-compat.json): migration 0291 copies the per-kind
// sidebar layout into the team list and leaves the per-kind rows alone, so
// a client that still reads them sees the same groups, folders and order
// before and after, and the team list's projection keeps showing it so.

const ready = await probeRealDb(`SELECT EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'sidebar_layout') AS ok`)
if (!ready.ok) console.info(`[sidebar-compat-migration.real-db] skipped: ${ready.reason}`)

const MIGRATION = new URL(`./out/0291_exp2027_sidebar_compat.sql`, import.meta.url)
const copyStatement = () => readFileSync(MIGRATION, `utf8`).split(`--> statement-breakpoint`).at(-1)!

const id8 = () => crypto.randomUUID().slice(0, 8)

type Row = SidebarEntry & { section: string }

async function rowsOf(tx: RealDbTx, userId: string): Promise<Row[]> {
  const result = await tx.execute<{
    id: string
    section: string
    kind: `folder` | `item`
    target_id: string | null
    parent_id: string | null
    name: string | null
    sort_order: number
  }>(sql`SELECT id, section, kind, target_id, parent_id, name, sort_order FROM sidebar_layout WHERE user_id = ${userId}`)
  return result.rows.map((row) => ({
    id: row.id,
    section: row.section,
    kind: row.kind,
    targetId: row.target_id,
    parentId: row.parent_id,
    name: row.name,
    sortOrder: Number(row.sort_order),
  }))
}

describe.skipIf(!ready.ok)(`migration 0291: an older client's sidebar survives`, () => {
  it(`keeps every per-kind row and the groups it shows, and the projection agrees`, async () => {
    await inRollback(async (tx) => {
      const userId = `compat-${id8()}`
      await tx.execute(sql`
        INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
        VALUES (${userId}, 'Tester', ${`${userId}@compat.test`}, true, false, now(), now())`)
      const team = await tx.execute<{ id: string }>(sql`INSERT INTO teams (name, slug) VALUES ('Compat', ${`compat-${id8()}`}) RETURNING id`)
      const teamId = team.rows[0]!.id
      const [b1, b2, b3, a1, a2, p1] = Array.from({ length: 6 }, () => crypto.randomUUID())
      for (const [id, prefix, order] of [[b1, `CA`, 1], [b2, `CB`, 2], [b3, `CC`, 3]] as const) {
        await tx.execute(sql`
          INSERT INTO boards (id, team_id, name, slug, prefix, sort_order)
          VALUES (${id}, ${teamId}, ${`Board ${prefix}`}, ${`board-${prefix.toLowerCase()}-${id8()}`}, ${prefix}, ${order})`)
      }
      for (const [id, order] of [[a1, 1], [a2, 2]] as const) {
        await tx.execute(sql`
          INSERT INTO agents (id, team_id, name, device_id, owner_id, sort_order)
          VALUES (${id}, ${teamId}, ${`Agent ${order}`}, 'dev', ${userId}, ${order})`)
      }
      await tx.execute(sql`INSERT INTO apps (id, team_id, slug, name) VALUES (${p1}, ${teamId}, ${`app-${id8()}`}, 'Calendar')`)

      // What an older client wrote: a boards folder holding b1, b3 above it,
      // b2 unplaced; agents: a2 first; apps untouched.
      const folderId = crypto.randomUUID()
      await tx.execute(sql`
        INSERT INTO sidebar_layout (id, user_id, team_id, section, kind, target_id, parent_id, name, sort_order) VALUES
          (${folderId}, ${userId}, ${teamId}, 'board', 'folder', NULL, NULL, 'Ops', 2),
          (gen_random_uuid(), ${userId}, ${teamId}, 'board', 'item', ${b1}, ${folderId}, NULL, 1),
          (gen_random_uuid(), ${userId}, ${teamId}, 'board', 'item', ${b3}, NULL, NULL, 1),
          (gen_random_uuid(), ${userId}, ${teamId}, 'agent', 'item', ${a2}, NULL, NULL, 1)`)
      const before = await rowsOf(tx, userId)

      await tx.execute(sql.raw(copyStatement()))
      const after = await rowsOf(tx, userId)

      // Every per-kind row is still there, unchanged but for the folder's link.
      const perKind = after.filter((row) => row.section !== `team`)
      const teamRows = after.filter((row) => row.section === `team`)
      const teamFolder = teamRows.find((row) => row.kind === `folder`)!
      expect(teamFolder.name).toBe(`Ops`)
      expect(perKind).toEqual(
        expect.arrayContaining(before.map((row) => (row.id === folderId ? { ...row, targetId: teamFolder.id } : row)))
      )
      expect(perKind).toHaveLength(before.length)

      // The team list holds every target once: the per-kind order, then the rest.
      const kinds = new Map<string, SidebarTargetKind>([
        [b1, `board`], [b2, `board`], [b3, `board`], [a1, `agent`], [a2, `agent`], [p1, `app`],
      ])
      const defaults: Record<SidebarCompatKind, string[]> = { board: [b1, b2, b3], agent: [a1, a2], app: [p1] }
      expect(sidebarTree([b1, b2, b3, a1, a2, p1], teamRows)).toEqual([
        { kind: `item`, id: b3 },
        { kind: `folder`, id: teamFolder.id, name: `Ops`, children: [b1] },
        { kind: `item`, id: b2 },
        { kind: `item`, id: a2 },
        { kind: `item`, id: a1 },
        { kind: `item`, id: p1 },
      ])

      // Each kind's group reads the same before, after, and after the
      // server's next projection of the team list.
      for (const kind of [`board`, `agent`, `app`] as const) {
        const old = before.filter((row) => row.section === kind)
        const now = perKind.filter((row) => row.section === kind)
        const writes = projectTeamToKind({
          team: teamRows,
          kindOf: (id) => kinds.get(id) ?? null,
          kind,
          existing: now,
          mint: () => crypto.randomUUID(),
        })
        const deleted = new Set(writes.deletes)
        const projected: SidebarEntry[] = [
          ...now.filter((row) => !deleted.has(row.id) && !writes.upserts.some((w) => w.id === row.id)),
          ...writes.upserts.map((w, i) => ({ ...w, id: w.id ?? `new-${i}` })),
        ]
        const view = (rows: readonly SidebarEntry[]) => sidebarTree(defaults[kind], rows)
        expect(view(now), kind).toEqual(view(old))
        expect(view(projected), kind).toEqual(view(old))
      }

      // The older client then drags b2 into its folder: the team list
      // follows, and every group is projected from it again.
      await tx.execute(sql`
        INSERT INTO sidebar_layout (user_id, team_id, section, kind, target_id, parent_id, sort_order)
        VALUES (${userId}, ${teamId}, 'board', 'item', ${b2}, ${folderId}, 2)
        ON CONFLICT (user_id, section, target_id) WHERE target_id IS NOT NULL
        DO UPDATE SET parent_id = excluded.parent_id, sort_order = excluded.sort_order`)
      await syncSidebarSections(tx, userId, teamId, `board`)
      const synced = await rowsOf(tx, userId)
      const syncedTeam = synced.filter((row) => row.section === `team`)
      expect(sidebarTree([b1, b2, b3, a1, a2, p1], syncedTeam)).toEqual([
        { kind: `item`, id: b3 },
        { kind: `folder`, id: teamFolder.id, name: `Ops`, children: [b1, b2] },
        { kind: `item`, id: a2 },
        { kind: `item`, id: a1 },
        { kind: `item`, id: p1 },
      ])
      expect(sidebarTree(defaults.board, synced.filter((row) => row.section === `board`))).toEqual([
        { kind: `item`, id: b3 },
        { kind: `folder`, id: folderId, name: `Ops`, children: [b1, b2] },
      ])

      // A new client renames the team folder: the boards group follows.
      await tx.execute(sql`UPDATE sidebar_layout SET name = 'Ops desk' WHERE id = ${teamFolder.id}`)
      await syncSidebarSections(tx, userId, teamId, `team`)
      const renamed = await rowsOf(tx, userId)
      expect(renamed.find((row) => row.id === folderId)?.name).toBe(`Ops desk`)
    })
  })
})
