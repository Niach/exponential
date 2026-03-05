import { z } from "zod"
import { router, procedure, authedProcedure, generateTxId } from "@/lib/trpc"
import { db } from "@/db/connection"
import { workspaceInvites, workspaceMembers, workspaces } from "@/db/schema"
import { and, eq, isNull } from "drizzle-orm"
import { randomBytes } from "crypto"
import { TRPCError } from "@trpc/server"
import { assertWorkspaceMember } from "@/lib/workspace-membership"

export const workspaceInvitesRouter = router({
  create: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        role: z.enum([`owner`, `member`]).default(`member`),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertWorkspaceMember(ctx.session.user.id, input.workspaceId, [
        `owner`,
      ])

      const token = randomBytes(32).toString(`hex`)
      const expiresAt = new Date(Date.now() + 7 * 24 * 60 * 60 * 1000) // 7 days

      const [invite] = await db
        .insert(workspaceInvites)
        .values({
          workspaceId: input.workspaceId,
          invitedById: ctx.session.user.id,
          role: input.role,
          token,
          expiresAt,
        })
        .returning()

      return { invite, token }
    }),

  accept: authedProcedure
    .input(z.object({ token: z.string() }))
    .mutation(async ({ ctx, input }) => {
      return await db.transaction(async (tx) => {
        const [invite] = await tx
          .select()
          .from(workspaceInvites)
          .where(eq(workspaceInvites.token, input.token))
          .limit(1)

        if (!invite) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Invite not found`,
          })
        }

        if (invite.acceptedAt) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Invite has already been used`,
          })
        }

        if (invite.expiresAt < new Date()) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Invite has expired`,
          })
        }

        // Check if already a member
        const [existing] = await tx
          .select()
          .from(workspaceMembers)
          .where(
            and(
              eq(workspaceMembers.workspaceId, invite.workspaceId),
              eq(workspaceMembers.userId, ctx.session.user.id)
            )
          )
          .limit(1)

        const [workspace] = await tx
          .select()
          .from(workspaces)
          .where(eq(workspaces.id, invite.workspaceId))
          .limit(1)

        // Mark invite as accepted
        await tx
          .update(workspaceInvites)
          .set({ acceptedAt: new Date() })
          .where(eq(workspaceInvites.id, invite.id))

        if (existing) {
          return { workspace, alreadyMember: true }
        }

        const txId = await generateTxId(tx)

        // Create membership
        await tx.insert(workspaceMembers).values({
          workspaceId: invite.workspaceId,
          userId: ctx.session.user.id,
          role: invite.role,
        })

        return { workspace, alreadyMember: false, txId }
      })
    }),

  list: authedProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertWorkspaceMember(ctx.session.user.id, input.workspaceId)

      const invites = await db
        .select()
        .from(workspaceInvites)
        .where(
          and(
            eq(workspaceInvites.workspaceId, input.workspaceId),
            isNull(workspaceInvites.acceptedAt)
          )
        )

      return { invites }
    }),

  revoke: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const [invite] = await db
        .select()
        .from(workspaceInvites)
        .where(eq(workspaceInvites.id, input.id))
        .limit(1)

      if (!invite) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Invite not found` })
      }

      await assertWorkspaceMember(ctx.session.user.id, invite.workspaceId, [
        `owner`,
      ])

      await db.delete(workspaceInvites).where(eq(workspaceInvites.id, input.id))

      return { success: true }
    }),

  getByToken: procedure
    .input(z.object({ token: z.string() }))
    .query(async ({ input }) => {
      const [invite] = await db
        .select({
          id: workspaceInvites.id,
          workspaceId: workspaceInvites.workspaceId,
          role: workspaceInvites.role,
          acceptedAt: workspaceInvites.acceptedAt,
          expiresAt: workspaceInvites.expiresAt,
          workspaceName: workspaces.name,
        })
        .from(workspaceInvites)
        .innerJoin(workspaces, eq(workspaceInvites.workspaceId, workspaces.id))
        .where(eq(workspaceInvites.token, input.token))
        .limit(1)

      if (!invite) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Invite not found` })
      }

      return { invite }
    }),
})
