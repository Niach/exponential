import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, desc, eq, inArray } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { db } from "@/db/connection"
import {
  emailDeliveries,
  issues,
  projects,
  supportMessages,
  supportThreads,
} from "@/db/schema"
import {
  assertWorkspaceMember,
  getProjectWorkspaceId,
} from "@/lib/workspace-membership"
import { recordIssueEvent } from "@/lib/integrations/activity"
import {
  fireAndForgetReporterResolution,
  fireAndForgetStatusChangeNotify,
} from "@/lib/integrations/notifications"
import { sendSupportReplyEmail } from "@/lib/email"
import {
  MAX_SUPPORT_MESSAGE_CHARS,
  latestMessagesByThread,
  regenerateThreadToken,
  revokeThreadToken,
  supportThreadUrl,
} from "@/lib/helpdesk/service"

const messageBodySchema = z
  .string()
  .trim()
  .min(1)
  .max(MAX_SUPPORT_MESSAGE_CHARS)

// Helpdesk-resolved == the underlying issue reached a terminal status.
const RESOLVED_STATUSES = [`done`, `cancelled`, `duplicate`] as const

// Load a thread and gate on membership of its project's workspace. Every
// member handles support (permissions collapsed to membership-only) — no
// owner gating anywhere in this router.
async function loadThreadForMember(userId: string, threadId: string) {
  const [thread] = await db
    .select()
    .from(supportThreads)
    .where(eq(supportThreads.id, threadId))
    .limit(1)
  if (!thread) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Thread not found` })
  }
  const project = await getProjectWorkspaceId(thread.projectId)
  await assertWorkspaceMember(userId, project.workspaceId)
  return { thread, workspaceId: project.workspaceId }
}

export const helpdeskRouter = router({
  // The inbox list: one row per thread across the workspace's helpdesk
  // projects (optionally narrowed to one), filtered open/resolved via the
  // underlying issue status, newest activity first. `unread` = the reporter
  // spoke last — there is no per-member read state in the MVP.
  listThreads: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        projectId: z.string().uuid().optional(),
        filter: z.enum([`open`, `resolved`]).default(`open`),
      })
    )
    .query(async ({ ctx, input }) => {
      await assertWorkspaceMember(ctx.session.user.id, input.workspaceId)

      const statusFilter =
        input.filter === `resolved`
          ? inArray(issues.status, [...RESOLVED_STATUSES])
          : inArray(issues.status, [`backlog`, `todo`, `in_progress`])

      const rows = await ctx.db
        .select({
          id: supportThreads.id,
          issueId: supportThreads.issueId,
          projectId: supportThreads.projectId,
          reporterEmail: supportThreads.reporterEmail,
          reporterName: supportThreads.reporterName,
          lastReporterSeenAt: supportThreads.lastReporterSeenAt,
          createdAt: supportThreads.createdAt,
          updatedAt: supportThreads.updatedAt,
          issueIdentifier: issues.identifier,
          issueTitle: issues.title,
          issueStatus: issues.status,
          projectName: projects.name,
        })
        .from(supportThreads)
        .innerJoin(issues, eq(issues.id, supportThreads.issueId))
        .innerJoin(projects, eq(projects.id, supportThreads.projectId))
        .where(
          and(
            eq(projects.workspaceId, input.workspaceId),
            input.projectId
              ? eq(supportThreads.projectId, input.projectId)
              : undefined,
            statusFilter
          )
        )
        .orderBy(desc(supportThreads.updatedAt))

      const latest = await latestMessagesByThread(rows.map((row) => row.id))
      return rows.map((row) => {
        const last = latest.get(row.id) ?? null
        return {
          ...row,
          lastMessage: last,
          unread: last?.direction === `inbound`,
        }
      })
    }),

  // Full conversation including internal notes (member-only surface).
  getThread: authedProcedure
    .input(z.object({ threadId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const { thread } = await loadThreadForMember(
        ctx.session.user.id,
        input.threadId
      )
      const messages = await ctx.db
        .select()
        .from(supportMessages)
        .where(eq(supportMessages.threadId, thread.id))
        .orderBy(supportMessages.createdAt)
      const [issue] = await ctx.db
        .select({
          id: issues.id,
          identifier: issues.identifier,
          title: issues.title,
          status: issues.status,
          priority: issues.priority,
          assigneeId: issues.assigneeId,
        })
        .from(issues)
        .where(eq(issues.id, thread.issueId))
        .limit(1)
      return { thread, messages, issue: issue ?? null }
    }),

  // Public reply: insert the outbound message and email the reporter. The
  // email's magic link needs the RAW token and only its hash is stored, so
  // every reply email ROTATES the token — the freshest email always holds the
  // working link, older links politely expire (the anonymous page explains).
  reply: authedProcedure
    .input(
      z.object({ threadId: z.string().uuid(), body: messageBodySchema })
    )
    .mutation(async ({ ctx, input }) => {
      const { thread } = await loadThreadForMember(
        ctx.session.user.id,
        input.threadId
      )

      const { message, rawToken } = await ctx.db.transaction(async (tx) => {
        const [inserted] = await tx
          .insert(supportMessages)
          .values({
            threadId: thread.id,
            issueId: thread.issueId,
            authorUserId: ctx.session.user.id,
            direction: `outbound`,
            visibility: `public`,
            body: input.body,
          })
          .returning()
        // Rotation preserves revocation: a reply on a closed thread mails a
        // readable (but still read-only) link — reopening is explicit.
        const { rawToken } = await regenerateThreadToken(tx, thread.id, {
          clearRevocation: false,
        })
        await tx
          .update(supportThreads)
          .set({ updatedAt: new Date() })
          .where(eq(supportThreads.id, thread.id))
        return { message: inserted, rawToken }
      })

      const [projectRow] = await ctx.db
        .select({ name: projects.name })
        .from(projects)
        .where(eq(projects.id, thread.projectId))
        .limit(1)

      // Email outside the transaction; a failed send never loses the message.
      // The delivery ledger row deliberately stores NO thread URL (the raw
      // token must not be persisted anywhere).
      try {
        const result = await sendSupportReplyEmail({
          to: thread.reporterEmail,
          projectName: projectRow?.name ?? `the team`,
          replyText: input.body,
          threadUrl: supportThreadUrl(rawToken),
        })
        const [delivery] = await ctx.db
          .insert(emailDeliveries)
          .values({
            userId: null,
            toEmail: thread.reporterEmail,
            issueId: thread.issueId,
            kind: `support_reply`,
            status: result.delivered ? `sent` : `failed`,
            provider: result.provider,
            providerMessageId: result.messageId,
            sentAt: result.delivered ? new Date() : null,
          })
          .returning({ id: emailDeliveries.id })
        if (delivery) {
          await ctx.db
            .update(supportMessages)
            .set({ emailDeliveryId: delivery.id })
            .where(eq(supportMessages.id, message.id))
        }
      } catch (err) {
        console.error(`[helpdesk] reply email failed:`, err)
      }

      return { message }
    }),

  // Internal note: member-only annotation. Never emailed, never visible on
  // the anonymous page, no token churn.
  note: authedProcedure
    .input(
      z.object({ threadId: z.string().uuid(), body: messageBodySchema })
    )
    .mutation(async ({ ctx, input }) => {
      const { thread } = await loadThreadForMember(
        ctx.session.user.id,
        input.threadId
      )
      const [message] = await ctx.db
        .insert(supportMessages)
        .values({
          threadId: thread.id,
          issueId: thread.issueId,
          authorUserId: ctx.session.user.id,
          direction: `outbound`,
          visibility: `internal`,
          body: input.body,
        })
        .returning()
      return { message }
    }),

  // Close: terminal issue status + revoke the magic link (transcript stays
  // readable, replies rejected). Delegates status semantics to the same
  // derivations issues.update applies.
  close: authedProcedure
    .input(z.object({ threadId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const { thread, workspaceId } = await loadThreadForMember(
        ctx.session.user.id,
        input.threadId
      )
      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [current] = await tx
          .select({ status: issues.status })
          .from(issues)
          .where(eq(issues.id, thread.issueId))
          .limit(1)
        if (!current) {
          throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
        }
        let statusChange: { from: string; to: string } | null = null
        if (
          !(RESOLVED_STATUSES as readonly string[]).includes(current.status)
        ) {
          await tx
            .update(issues)
            .set({ status: `done`, completedAt: new Date() })
            .where(eq(issues.id, thread.issueId))
          await recordIssueEvent(tx, {
            issueId: thread.issueId,
            workspaceId,
            actorUserId: ctx.session.user.id,
            type: `status_changed`,
            payload: { from: current.status, to: `done` },
          })
          statusChange = { from: current.status, to: `done` }
        }
        await revokeThreadToken(tx, thread.id)
        return { txId, statusChange }
      })
      if (result.statusChange) {
        fireAndForgetStatusChangeNotify({
          issueId: thread.issueId,
          actorUserId: ctx.session.user.id,
          fromStatus: result.statusChange.from,
          toStatus: result.statusChange.to,
        })
        fireAndForgetReporterResolution({
          issueId: thread.issueId,
          toStatus: result.statusChange.to,
        })
      }
      return { ok: true as const, txId: result.txId }
    }),

  // Reopen: issue back to todo + a FRESH magic link (the revoked one stays
  // dead). The new link reaches the reporter with the next reply email.
  reopen: authedProcedure
    .input(z.object({ threadId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const { thread, workspaceId } = await loadThreadForMember(
        ctx.session.user.id,
        input.threadId
      )
      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [current] = await tx
          .select({ status: issues.status })
          .from(issues)
          .where(eq(issues.id, thread.issueId))
          .limit(1)
        if (!current) {
          throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
        }
        let statusChange: { from: string; to: string } | null = null
        if ((RESOLVED_STATUSES as readonly string[]).includes(current.status)) {
          await tx
            .update(issues)
            .set({ status: `todo`, completedAt: null, duplicateOfId: null })
            .where(eq(issues.id, thread.issueId))
          await recordIssueEvent(tx, {
            issueId: thread.issueId,
            workspaceId,
            actorUserId: ctx.session.user.id,
            type: `status_changed`,
            payload: { from: current.status, to: `todo` },
          })
          statusChange = { from: current.status, to: `todo` }
        }
        await regenerateThreadToken(tx, thread.id, { clearRevocation: true })
        return { txId, statusChange }
      })
      if (result.statusChange) {
        fireAndForgetStatusChangeNotify({
          issueId: thread.issueId,
          actorUserId: ctx.session.user.id,
          fromStatus: result.statusChange.from,
          toStatus: result.statusChange.to,
        })
      }
      return { ok: true as const, txId: result.txId }
    }),
})
