import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray, isNull, sql } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import {
  AGENT_INSTRUCTIONS_MAX_BYTES,
  agentFacetsSchema,
  agentTriggersSchema,
  boardIconSchema,
  codingSessionBlockedSchema,
  hexColorSchema,
  MESSAGE_BODY_MAX,
  messageCardSchema,
  questionCardOptionSchema,
  sessionsSendInputSchema,
  sessionStatusSchema,
  type CodingSessionBlocked,
  type MessageCard,
  type SessionsSendResult,
} from "@exp/db-schema/domain"
import { attachments, messages, sessionIssues, sessions, type Session } from "@/db/schema"
import { assertTeamOwner } from "@/lib/auth/membership"
import {
  assertCanSteer,
  assertDeviceHost,
  loadSessionForUser,
} from "@/lib/sessions/access"
import { composeSessionBrief } from "@/lib/sessions/brief"
import { answeredCard } from "@/lib/sessions/card-answer"
import { linkIssues, unlinkIssue } from "@/lib/sessions/issues"
import {
  endSession,
  relayKillSessionsBestEffort,
  stopSession,
} from "@/lib/sessions/lifecycle"
import { postAgentMessage } from "@/lib/sessions/messages"
import { resolveSessionDevice, sendSession, withTriggerIds } from "@/lib/sessions/send"
import {
  fireAndForgetSessionMessageNotify,
  notifySessionBlocked,
} from "@/lib/integrations/notifications"

// EXP-1291 Threads v2: THE session router. A thread IS a root session; a
// team agent is a root of kind `agent`; children are sub-sessions. ONE
// entrypoint starts and messages them all: `sessions.send` (the composer,
// Start on an issue, an action, a trigger firing, an agent spawn, a
// follow-up). The database is the bus: a new session is a `queued` row
// naming its device, a message a `messages` row; the device picks both up
// off the `sessions` / `messages` shapes and reports back through the
// DEVICE procedures below (claim/report/reply/consume). Reads ride the
// shapes; tRPC returns ids + txids only.

const uuid = z.string().uuid()

/** The cards a DEVICE may post (the server owns event/session cards). */
const DEVICE_CARD_KINDS = new Set([`question`, `plan`, `subagent`, `picture`])

/** EXP-1322: a question card's options: labels or `{label, description}`. */
const questionCardOptionsSchema = z.array(questionCardOptionSchema).max(50)

/** Sidecar to plain-label `options` (iOS <= 0.14.59 drops object options):
 *  one description or null per option, parallel to `options`. Stored on the
 *  card as posted (messageCardSchema is passthrough); the type of each entry
 *  is checked, the length is left to the device. */
const questionCardOptionDescriptionsSchema = z
  .array(z.string().max(2000).nullable())
  .max(50)

async function loadAgent(userId: string, sessionId: string): Promise<Session> {
  const agent = await loadSessionForUser(userId, sessionId)
  if (agent.kind !== `agent`) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Agent not found` })
  }
  return agent
}

type Database = typeof import("@/db/connection").db

const cardAnswerInput = z.object({
  messageId: uuid,
  answer: z.string().max(MESSAGE_BODY_MAX).optional(),
  decision: z.enum([`approved`, `rejected`]).optional(),
  feedback: z.string().max(MESSAGE_BODY_MAX).optional(),
})

/** The card message + its session (steer-gated) + the answered card. */
async function loadCardForAnswer(
  database: Database,
  userId: string,
  input: z.infer<typeof cardAnswerInput>
) {
  const [message] = await database.select().from(messages).where(eq(messages.id, input.messageId)).limit(1)
  if (!message) throw new TRPCError({ code: `NOT_FOUND`, message: `Message not found` })
  const session = await loadSessionForUser(userId, message.sessionId)
  await assertCanSteer(userId, session)
  const answered = answeredCard(message.card as MessageCard | null, input)
  if (!answered.ok) throw new TRPCError({ code: answered.code, message: answered.message })
  return { message, session, card: answered.card }
}

/** Stamp `card` + `answered_at` while still unanswered; needs-you clears
 *  once nothing else waits on a person. `claimed` = this call stamped it. */
async function stampAnsweredCard(
  database: Database,
  messageId: string,
  sessionId: string,
  card: MessageCard
): Promise<{ claimed: boolean; txid: number }> {
  return database.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const now = new Date()
    const claimed = await tx
      .update(messages)
      .set({ card, answeredAt: now, updatedAt: now })
      .where(and(eq(messages.id, messageId), isNull(messages.answeredAt)))
      .returning({ id: messages.id })
    if (claimed.length === 0) return { claimed: false, txid }
    const [{ pending } = { pending: 0 }] = await tx
      .select({ pending: sql<number>`count(*)::int` })
      .from(messages)
      .where(
        and(
          eq(messages.sessionId, sessionId),
          isNull(messages.answeredAt),
          sql`${messages.card}->>'kind' IN ('question', 'plan')`
        )
      )
    if (Number(pending) === 0) {
      await tx
        .update(sessions)
        .set({ needsInput: false, updatedAt: now })
        .where(eq(sessions.id, sessionId))
    }
    return { claimed: true, txid }
  })
}

export const sessionsRouter = router({
  // ── people (and agents over MCP) ──────────────────────────────────────────

  /** THE entrypoint. → { sessionId, messageId, txid }. */
  send: authedProcedure
    .input(sessionsSendInputSchema)
    .mutation(({ ctx, input }): Promise<SessionsSendResult & { txid: number }> =>
      sendSession({ userId: ctx.session.user.id, input })
    ),

  /** Interrupt the session's current turn (it stays live, goes idle). Every
   *  member may Stop a SHARED session; a private one only its starter. */
  stop: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .mutation(({ ctx, input }) =>
      stopSession({ userId: ctx.session.user.id, sessionId: input.sessionId })
    ),

  /** End a session explicitly (status `ended`; agents refuse). Ending a
   *  ROOT ends its whole tree. */
  end: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .mutation(async ({ ctx, input }): Promise<{ txid: number }> => {
      const { txid } = await endSession({
        userId: ctx.session.user.id,
        sessionId: input.sessionId,
      })
      return { txid }
    }),

  // COMPAT (EXP-1316): Android <= 0.14.61 and desktop <= 0.14.69 rename a
  // thread by hand through this procedure. The stored `sessions.title` now
  // ranks BELOW the agent/action/issue names in the contract's threadTitle
  // ladder, so the write is harmless. Delete once
  // CLIENT_MIN_VERSION_ANDROID >= 0.14.62 AND CLIENT_MIN_VERSION_DESKTOP >= 0.14.70.
  setTitle: authedProcedure
    .input(z.object({ sessionId: uuid, title: z.string().trim().max(255).nullable() }))
    .mutation(async ({ ctx, input }): Promise<{ txid: number }> => {
      const userId = ctx.session.user.id
      const session = await loadSessionForUser(userId, input.sessionId)
      await assertCanSteer(userId, session)
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        await tx
          .update(sessions)
          .set({ title: input.title || null, updatedAt: new Date() })
          .where(eq(sessions.id, session.id))
        return { txid }
      })
    }),

  /** Link issues to a thread (always lands on the ROOT; the header "+"). */
  linkIssues: authedProcedure
    .input(z.object({ sessionId: uuid, issueIds: z.array(uuid).min(1).max(30) }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const session = await loadSessionForUser(userId, input.sessionId)
      await assertCanSteer(userId, session)
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        await linkIssues(tx, {
          rootId: session.rootId,
          issueIds: input.issueIds,
          source: `user`,
          addedBy: userId,
          addedBySessionId: null,
        })
        return { txid }
      })
    }),

  unlinkIssue: authedProcedure
    .input(z.object({ sessionId: uuid, issueId: uuid }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const session = await loadSessionForUser(userId, input.sessionId)
      await assertCanSteer(userId, session)
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        await unlinkIssue(tx, { rootId: session.rootId, issueId: input.issueId })
        return { txid }
      })
    }),

  /** Answer a `question` card (text) or decide a `plan` card IN the card;
   *  the device delivers it to the agent off the messages shape. */
  answer: authedProcedure
    .input(cardAnswerInput)
    .mutation(async ({ ctx, input }) => {
      const { message, session, card } = await loadCardForAnswer(ctx.db, ctx.session.user.id, input)
      if (message.answeredAt) {
        throw new TRPCError({ code: `CONFLICT`, message: `Already answered` })
      }
      // The answer lives IN the card (answered_at flips): the device feeds it
      // to the agent off the messages shape, so no person message is posted.
      const result = await stampAnsweredCard(ctx.db, message.id, session.id, card)
      if (!result.claimed) {
        throw new TRPCError({ code: `CONFLICT`, message: `Already answered` })
      }
      return { txid: result.txid }
    }),

  /** EXP-1316: the DEVICE's write-back when a question/plan card was
   *  answered in the session view (over the relay), so the thread card
   *  closes too. Same card shape and auth as `answer`; idempotent (an
   *  answered card = ok, never CONFLICT). Enqueues nothing: the engine
   *  already resolved the question, so it left the engine's open cards and
   *  the device's pickup never feeds it again. */
  resolveCard: authedProcedure
    .input(cardAnswerInput)
    .mutation(async ({ ctx, input }): Promise<{ ok: true; txid?: number }> => {
      const { message, session, card } = await loadCardForAnswer(ctx.db, ctx.session.user.id, input)
      if (message.answeredAt) return { ok: true }
      const result = await stampAnsweredCard(ctx.db, message.id, session.id, card)
      return result.claimed ? { ok: true, txid: result.txid } : { ok: true }
    }),

  /** Revoke a person's own message the device has not consumed yet. Its
   *  files fall back to owner-less uploads the orphan sweep reclaims. */
  unqueue: authedProcedure
    .input(z.object({ messageId: uuid }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        await tx
          .update(attachments)
          .set({ messageId: null })
          .where(eq(attachments.messageId, input.messageId))
        const deleted = await tx
          .delete(messages)
          .where(
            and(
              eq(messages.id, input.messageId),
              eq(messages.authorKind, `user`),
              eq(messages.authorId, userId),
              isNull(messages.consumedAt)
            )
          )
          .returning({ id: messages.id })
        if (deleted.length === 0) {
          throw new TRPCError({
            code: `CONFLICT`,
            message: `The message was already delivered`,
          })
        }
        return { txid }
      })
    }),

  // ── team agents (Settings → Agents; owner writes, members read) ───────────
  // Create = `sessions.send({teamId, deviceId, account?, agent})`.

  /** The agent row's server-only facets (instructions) for the settings page. */
  getAgent: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .query(async ({ ctx, input }): Promise<{ sessionId: string; instructions: string }> => {
      const agent = await loadAgent(ctx.session.user.id, input.sessionId)
      return { sessionId: agent.id, instructions: agent.instructions ?? `` }
    }),

  updateAgent: authedProcedure
    .input(
      z.object({
        sessionId: uuid,
        name: agentFacetsSchema.shape.name.optional(),
        icon: boardIconSchema.optional(),
        color: hexColorSchema.optional(),
        // NOT the facet schema's (its `.default('')` would wipe the
        // instructions on every update that leaves them out).
        instructions: z.string().max(AGENT_INSTRUCTIONS_MAX_BYTES).optional(),
        triggers: agentTriggersSchema.optional(),
        deviceId: z.string().min(1).max(128).optional(),
        account: z.string().min(1).max(64).nullable().optional(),
        model: z.string().max(64).nullable().optional(),
        sortOrder: z.number().finite().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const agent = await loadAgent(userId, input.sessionId)
      await assertTeamOwner(userId, agent.teamId)
      // A new device re-stamps the agent tree's visibility (its device's
      // share decides it) and hands the agent over: queued again, the new
      // device boots it, the old one drops a row that no longer names it.
      const moved =
        input.deviceId !== undefined && input.deviceId !== agent.deviceId
          ? await resolveSessionDevice(userId, agent.teamId, input.deviceId)
          : null
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const now = new Date()
        await tx
          .update(sessions)
          .set({
            ...(input.name !== undefined ? { name: input.name } : {}),
            ...(input.icon !== undefined ? { icon: input.icon } : {}),
            ...(input.color !== undefined ? { color: input.color } : {}),
            ...(input.instructions !== undefined ? { instructions: input.instructions } : {}),
            ...(input.triggers !== undefined ? { triggers: withTriggerIds(input.triggers) } : {}),
            ...(input.account !== undefined ? { account: input.account } : {}),
            ...(input.model !== undefined ? { model: input.model } : {}),
            ...(input.sortOrder !== undefined ? { sortOrder: input.sortOrder } : {}),
            ...(moved
              ? {
                  deviceId: input.deviceId,
                  hostUserId: moved.hostUserId,
                  shared: moved.shared,
                  status: `queued` as const,
                  busy: false,
                }
              : {}),
            updatedAt: now,
          })
          .where(eq(sessions.id, agent.id))
        if (moved && moved.shared !== agent.shared) {
          await tx
            .update(sessions)
            .set({ shared: moved.shared, updatedAt: now })
            .where(eq(sessions.rootId, agent.id))
          await tx
            .update(messages)
            .set({ shared: moved.shared })
            .where(eq(messages.rootId, agent.id))
          await tx
            .update(sessionIssues)
            .set({ shared: moved.shared })
            .where(eq(sessionIssues.sessionId, agent.id))
        }
        return { txid }
      })
    }),

  deleteAgent: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const agent = await loadAgent(userId, input.sessionId)
      await assertTeamOwner(userId, agent.teamId)
      const result = await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const tree = await tx
          .select({ id: sessions.id })
          .from(sessions)
          .where(eq(sessions.rootId, agent.id))
        // Message files go back to owner-less uploads (the orphan sweep
        // reclaims their blobs) before the cascade drops the messages.
        await tx
          .update(attachments)
          .set({ messageId: null })
          .where(
            inArray(
              attachments.messageId,
              tx.select({ id: messages.id }).from(messages).where(eq(messages.rootId, agent.id))
            )
          )
        await tx.delete(sessions).where(eq(sessions.id, agent.id))
        return { txid, ids: tree.map((row) => row.id) }
      })
      await relayKillSessionsBestEffort(result.ids)
      return { txid: result.txid }
    }),

  // ── the device (the daemon / desktop that runs the session) ───────────────

  /** The server-composed first prompt (fixtures/session-brief.json) and the
   *  system-append layers: runbook + team prompt + agent instructions. */
  brief: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .query(async ({ ctx, input }) => {
      const session = await loadSessionForUser(ctx.session.user.id, input.sessionId)
      assertDeviceHost(ctx.session.user.id, session)
      return composeSessionBrief(session.id)
    }),

  /** queued → running: the device took it (idempotent for its own device). */
  claim: authedProcedure
    .input(z.object({ sessionId: uuid, branch: z.string().max(255).optional() }))
    .mutation(async ({ ctx, input }) => {
      const session = await loadSessionForUser(ctx.session.user.id, input.sessionId)
      assertDeviceHost(ctx.session.user.id, session)
      if (session.status === `ended`) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `This session has ended` })
      }
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const now = new Date()
        await tx
          .update(sessions)
          .set({
            ...(session.status === `queued` ? { status: `running` as const } : {}),
            startedAt: session.startedAt ?? now,
            ...(input.branch ? { branch: input.branch } : {}),
            updatedAt: now,
          })
          .where(eq(sessions.id, session.id))
        // The brief's Request IS the session's first person message: it is
        // delivered by the claim, so the device never feeds it twice.
        if (session.status === `queued`) {
          await tx.execute(sql`
            UPDATE ${messages} SET consumed_at = ${now}, updated_at = ${now}
            WHERE id = (
              SELECT id FROM ${messages}
              WHERE session_id = ${session.id} AND author_kind = 'user'
              ORDER BY created_at, id LIMIT 1
            ) AND consumed_at IS NULL`)
        }
        return { txid }
      })
    }),

  /** Device-written state: running ⇄ idle, busy, needs_input, blocked,
   *  branch, the CLI's auto-title; `ended` when the agent process is gone. */
  report: authedProcedure
    .input(
      z.object({
        sessionId: uuid,
        status: sessionStatusSchema.exclude([`queued`]).optional(),
        busy: z.boolean().optional(),
        needsInput: z.boolean().optional(),
        blocked: codingSessionBlockedSchema.nullable().optional(),
        branch: z.string().max(255).optional(),
        title: z.string().max(255).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const session = await loadSessionForUser(ctx.session.user.id, input.sessionId)
      assertDeviceHost(ctx.session.user.id, session)
      if (session.status === `ended`) {
        // An ended row stays ended; the device's late reports are no-ops.
        return ctx.db.transaction(async (tx) => ({ txid: await generateTxId(tx) }))
      }
      // An agent never ends: its process going away leaves it idle.
      const status =
        input.status === `ended` && session.kind === `agent` ? `idle` : input.status
      const ending = status === `ended`
      const now = new Date()
      const blocked = input.blocked as CodingSessionBlocked | null | undefined
      const newlyBlocked = !!blocked && !session.blocked
      const turnEnded =
        (input.busy === false && session.busy) || status === `idle` || ending
      const txid = await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        await tx
          .update(sessions)
          .set({
            ...(status ? { status } : {}),
            ...(ending
              ? { endedAt: now, busy: false, needsInput: false }
              : {
                  ...(input.busy !== undefined ? { busy: input.busy } : {}),
                  ...(input.needsInput !== undefined ? { needsInput: input.needsInput } : {}),
                }),
            ...(input.blocked !== undefined ? { blocked: blocked ?? null } : {}),
            ...(input.branch ? { branch: input.branch } : {}),
            // The CLI's auto-name only fills a blank title (a person's or
            // the agent's own title wins).
            ...(input.title?.trim() && !session.title ? { title: input.title.trim() } : {}),
            ...(turnEnded ? { lastTurnAt: now } : {}),
            updatedAt: now,
          })
          .where(eq(sessions.id, session.id))
        return txid
      })
      if (newlyBlocked) void notifySessionBlocked(session.id, blocked!)
      return { txid }
    }),

  /** The agent's side of the conversation: a turn's FINAL reply text and/or
   *  a card (question, plan, subagent, picture). */
  reply: authedProcedure
    .input(
      z.object({
        sessionId: uuid,
        body: z.string().max(MESSAGE_BODY_MAX).default(``),
        card: messageCardSchema.optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const session = await loadSessionForUser(ctx.session.user.id, input.sessionId)
      assertDeviceHost(ctx.session.user.id, session)
      const card = (input.card ?? null) as MessageCard | null
      if (card && !DEVICE_CARD_KINDS.has(card.kind)) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `A device cannot post ${card.kind} cards` })
      }
      if (
        card?.kind === `question` &&
        card.options !== undefined &&
        !questionCardOptionsSchema.safeParse(card.options).success
      ) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `Invalid question options` })
      }
      if (
        card?.kind === `question` &&
        (card as { optionDescriptions?: unknown }).optionDescriptions !== undefined &&
        !questionCardOptionDescriptionsSchema.safeParse(
          (card as { optionDescriptions?: unknown }).optionDescriptions
        ).success
      ) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `Invalid question option descriptions` })
      }
      if (!card && !input.body.trim()) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `A reply needs a body or a card` })
      }
      const asks = card?.kind === `question` || card?.kind === `plan`
      const result = await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const messageId = await postAgentMessage(tx, session, input.body, card)
        if (asks) {
          await tx
            .update(sessions)
            .set({ needsInput: true, updatedAt: new Date() })
            .where(eq(sessions.id, session.id))
        }
        return { messageId, txid }
      })
      if (input.body.includes(`@`)) fireAndForgetSessionMessageNotify({ messageId: result.messageId })
      return result
    }),

  /** The device delivered these person messages into the agent. */
  consume: authedProcedure
    .input(z.object({ sessionId: uuid, messageIds: z.array(uuid).min(1).max(100) }))
    .mutation(async ({ ctx, input }) => {
      const session = await loadSessionForUser(ctx.session.user.id, input.sessionId)
      assertDeviceHost(ctx.session.user.id, session)
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const now = new Date()
        await tx
          .update(messages)
          .set({ consumedAt: now, updatedAt: now })
          .where(
            and(
              eq(messages.sessionId, session.id),
              inArray(messages.id, input.messageIds),
              eq(messages.authorKind, `user`),
              isNull(messages.consumedAt)
            )
          )
        return { txid }
      })
    }),
})
