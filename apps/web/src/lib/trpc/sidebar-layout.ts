import { z } from "zod"
import { and, eq, inArray, sql } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import {
  SIDEBAR_FOLDER_NAME_MAX,
  SIDEBAR_SECTIONS,
  sidebarFolderName,
} from "@exp/domain-contract/sidebar-folders"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { sidebarLayout } from "@/db/schema"
import { resolveTeamAccess } from "@/lib/team-membership"

// #EXP-1832 (fixture `sidebar-folders.json`): the member's own sidebar layout.
// The client runs the pure `sidebarMove` / `sidebarUngroup` over its synced
// rows and sends the WRITES here (`save`); the server only checks they keep
// the layout's shape: rows are the caller's (the `user_id` predicate IS the
// authorization), a folder sits at the top level, an item's parent is a
// folder of the same section, only folders are deleted. Targets carry no FK
// (a board / agent / app the member cannot see never resolves on a client).

/** A section never holds more rows than a sidebar shows. */
export const SIDEBAR_LAYOUT_MAX_WRITES = 500

const sectionSchema = z.enum(SIDEBAR_SECTIONS as [`board`, `agent`, `app`])

const writeSchema = z.discriminatedUnion(`kind`, [
  z.object({
    kind: z.literal(`folder`),
    id: z.string().uuid(),
    targetId: z.null().optional(),
    parentId: z.null().optional(),
    name: z.string().max(SIDEBAR_FOLDER_NAME_MAX * 4).nullable().optional(),
    sortOrder: z.number().finite(),
  }),
  z.object({
    kind: z.literal(`item`),
    id: z.string().uuid().nullable().optional(),
    targetId: z.string().uuid(),
    parentId: z.string().uuid().nullable().optional(),
    name: z.null().optional(),
    sortOrder: z.number().finite(),
  }),
])

export const sidebarLayoutRouter = router({
  /** Write one move's rows (`sidebarMove` / `sidebarUngroup` output). */
  save: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        section: sectionSchema,
        upserts: z.array(writeSchema).max(SIDEBAR_LAYOUT_MAX_WRITES),
        deletes: z.array(z.string().uuid()).max(SIDEBAR_LAYOUT_MAX_WRITES),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      await resolveTeamAccess(userId, input.teamId, `read`)
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const existing = await tx
          .select({ id: sidebarLayout.id, kind: sidebarLayout.kind, teamId: sidebarLayout.teamId, section: sidebarLayout.section })
          .from(sidebarLayout)
          .where(eq(sidebarLayout.userId, userId))
        const mine = new Map(existing.map((row) => [row.id, row]))
        const deleted = new Set(input.deletes)
        const folderIds = new Set<string>()
        for (const row of existing) {
          if (row.kind === `folder` && row.teamId === input.teamId && row.section === input.section && !deleted.has(row.id)) folderIds.add(row.id)
        }
        for (const write of input.upserts) {
          if (write.kind !== `folder`) continue
          const row = mine.get(write.id)
          if (row && (row.kind !== `folder` || row.teamId !== input.teamId || row.section !== input.section)) {
            throw new TRPCError({ code: `BAD_REQUEST`, message: `Not a folder of this section` })
          }
          if (!row && (await tx.select({ id: sidebarLayout.id }).from(sidebarLayout).where(eq(sidebarLayout.id, write.id)).limit(1)).length > 0) {
            throw new TRPCError({ code: `BAD_REQUEST`, message: `Folder id taken` })
          }
          folderIds.add(write.id)
        }
        for (const write of input.upserts) {
          if (write.kind === `item` && write.parentId && !folderIds.has(write.parentId)) {
            throw new TRPCError({ code: `BAD_REQUEST`, message: `An item's parent must be a folder of this section` })
          }
        }
        for (const id of input.deletes) {
          const row = mine.get(id)
          if (row && (row.kind !== `folder` || row.teamId !== input.teamId || row.section !== input.section)) {
            throw new TRPCError({ code: `BAD_REQUEST`, message: `Only this section's folders are deleted` })
          }
        }

        for (const write of input.upserts) {
          if (write.kind === `folder`) {
            const name = sidebarFolderName(write.name ?? ``)
            await tx
              .insert(sidebarLayout)
              .values({ id: write.id, userId, teamId: input.teamId, section: input.section, kind: `folder`, name, sortOrder: write.sortOrder })
              .onConflictDoUpdate({
                target: sidebarLayout.id,
                set: { name, sortOrder: write.sortOrder, updatedAt: new Date() },
              })
          } else {
            // Raw SQL: the conflict target is a PARTIAL unique index
            // (uniq_sidebar_layout_user_target), whose predicate drizzle 0.39
            // drops from `targetWhere`.
            await tx.execute(sql`
              insert into sidebar_layout (user_id, team_id, section, kind, target_id, parent_id, sort_order)
              values (${userId}, ${input.teamId}, ${input.section}, 'item', ${write.targetId}, ${write.parentId ?? null}, ${write.sortOrder})
              on conflict (user_id, section, target_id) where target_id is not null
              do update set team_id = excluded.team_id, parent_id = excluded.parent_id, sort_order = excluded.sort_order, updated_at = now()
            `)
          }
        }
        if (input.deletes.length > 0) {
          await tx
            .delete(sidebarLayout)
            .where(
              and(
                eq(sidebarLayout.userId, userId),
                eq(sidebarLayout.kind, `folder`),
                inArray(sidebarLayout.id, input.deletes)
              )
            )
        }
        return { txId }
      })
    }),

  /** Rename one of the caller's folders (empty = the default name). */
  rename: authedProcedure
    .input(z.object({ id: z.string().uuid(), name: z.string().max(SIDEBAR_FOLDER_NAME_MAX * 4) }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const name = sidebarFolderName(input.name)
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const updated = await tx
          .update(sidebarLayout)
          .set({ name, updatedAt: new Date() })
          .where(and(eq(sidebarLayout.id, input.id), eq(sidebarLayout.userId, userId), eq(sidebarLayout.kind, `folder`)))
          .returning({ id: sidebarLayout.id })
        if (updated.length === 0) throw new TRPCError({ code: `NOT_FOUND`, message: `No such folder` })
        return { txId, name }
      })
    }),
})
