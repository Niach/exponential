import { z } from "zod"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { workspaceMembers } from "@/db/schema"
import { and, eq } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import {
  assertWorkspaceMember,
  assertNotPublicWorkspace,
  getWorkspaceById,
} from "@/lib/workspace-membership"
import { isUserAdmin } from "@/lib/admin"

/** Member management is allowed for a workspace owner OR a global admin. */
async function assertCanManageMembers(userId: string, workspaceId: string) {
  if (await isUserAdmin(userId)) {
    return
  }
  await assertWorkspaceMember(userId, workspaceId, [`owner`])
}

export const workspaceMembersRouter = router({
  // Self-service join, restricted to PUBLIC workspaces (private workspaces
  // require an invite). Membership is what makes a public board sync/appear
  // for a signed-in user — see getReadableWorkspaceIds. Idempotent so clients
  // can retry safely.
  join: authedProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const workspace = await getWorkspaceById(input.workspaceId)
      if (!workspace) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Workspace not found`,
        })
      }
      if (!workspace.isPublic) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Private workspaces require an invite`,
        })
      }

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [inserted] = await tx
          .insert(workspaceMembers)
          .values({
            workspaceId: input.workspaceId,
            userId: ctx.session.user.id,
            role: `member`,
          })
          .onConflictDoNothing()
          .returning()

        if (inserted) return { member: inserted, txId }

        const [existing] = await tx
          .select()
          .from(workspaceMembers)
          .where(
            and(
              eq(workspaceMembers.workspaceId, input.workspaceId),
              eq(workspaceMembers.userId, ctx.session.user.id)
            )
          )
          .limit(1)
        return { member: existing, txId }
      })
    }),

  updateRole: authedProcedure
    .input(
      z.object({
        memberId: z.string().uuid(),
        role: z.enum([`owner`, `member`]),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const [target] = await ctx.db
        .select()
        .from(workspaceMembers)
        .where(eq(workspaceMembers.id, input.memberId))
        .limit(1)

      if (!target) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Member not found` })
      }

      await assertNotPublicWorkspace(target.workspaceId, {
        message: `Membership on the public workspace cannot be modified`,
      })
      await assertCanManageMembers(ctx.session.user.id, target.workspaceId)

      // A workspace must always keep at least one owner — block demoting the
      // last one (mirrors the guard in `remove`).
      if (target.role === `owner` && input.role === `member`) {
        const owners = await ctx.db
          .select()
          .from(workspaceMembers)
          .where(
            and(
              eq(workspaceMembers.workspaceId, target.workspaceId),
              eq(workspaceMembers.role, `owner`)
            )
          )
        if (owners.length <= 1) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Cannot demote the last owner of a workspace`,
          })
        }
      }

      const [updated] = await ctx.db
        .update(workspaceMembers)
        .set({ role: input.role })
        .where(eq(workspaceMembers.id, input.memberId))
        .returning()

      return { member: updated }
    }),

  remove: authedProcedure
    .input(
      z.object({
        memberId: z.string().uuid(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const [target] = await ctx.db
        .select()
        .from(workspaceMembers)
        .where(eq(workspaceMembers.id, input.memberId))
        .limit(1)

      if (!target) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Member not found` })
      }

      // Public workspaces: joins are self-service, so leaves are too (and
      // instance admins may moderate). Owner-role management there still goes
      // through bootstrap/admin paths, never through this router's updateRole.
      const workspace = await getWorkspaceById(target.workspaceId)
      const isSelfRemove = target.userId === ctx.session.user.id
      if (workspace?.isPublic) {
        if (!isSelfRemove && !(await isUserAdmin(ctx.session.user.id))) {
          throw new TRPCError({
            code: `FORBIDDEN`,
            message: `Only the member themself or an admin can remove members from a public workspace`,
          })
        }
      } else if (!isSelfRemove) {
        await assertCanManageMembers(ctx.session.user.id, target.workspaceId)
      }

      // Prevent removing the last owner
      if (target.role === `owner`) {
        const owners = await ctx.db
          .select()
          .from(workspaceMembers)
          .where(
            and(
              eq(workspaceMembers.workspaceId, target.workspaceId),
              eq(workspaceMembers.role, `owner`)
            )
          )
        if (owners.length <= 1) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Cannot remove the last owner of a workspace`,
          })
        }
      }

      await ctx.db
        .delete(workspaceMembers)
        .where(eq(workspaceMembers.id, input.memberId))

      return { ok: true }
    }),
})
