import { isPersistent } from "@exp/domain-contract/session-kind"
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray, isNull, ne, or, sql } from "drizzle-orm"
import type { db } from "@/db/connection"
import { clientAtOrBelow } from "@/lib/client-version"
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
  PLAN_CARD_LABEL_MAX,
  planCardAnswersSchema,
  QUESTION_CARD_OPTIONS_MAX,
  questionCardOptionDescriptionsSchema,
  questionCardOptionSchema,
  sessionsConfigureInputSchema,
  sessionsSendInputSchema,
  sessionStatusSchema,
  type CodingSessionBlocked,
  type MessageCard,
  type SessionsSendResult,
} from "@exp/db-schema/domain"
import {
  attachments,
  messages,
  sessionIssues,
  sessionReads,
  sessions,
  type Session,
} from "@/db/schema"
import { assertTeamOwner } from "@/lib/auth/membership"
import {
  assertCanSteer,
  assertDeviceHost,
  loadSessionForUser,
} from "@/lib/sessions/access"
import { composeSessionBrief } from "@/lib/sessions/brief"
import { briefHandoff, completeHandoff, finishHandoff } from "@/lib/sessions/handoff"
import { configureSession, mcpPicks } from "@/lib/sessions/configure"
import { answeredCard } from "@/lib/sessions/card-answer"
import {
  issueIdentifiers,
  linkIssues,
  manualLinkText,
  unlinkIssue,
} from "@/lib/sessions/issues"
import {
  endSession,
  postReportToParent,
  settleFollowUpIssuesInTx,
  relayKillSessionsBestEffort,
  stopSession,
} from "@/lib/sessions/lifecycle"
import { parseMcpSessionHeader } from "@/lib/mcp/session-header"
import { normalizeCaption } from "@/lib/sessions/caption"
import { answeredOwnCard, pendingInput, postAgentMessage } from "@/lib/sessions/messages"
import {
  dismissReplies,
  dismissSuggestion,
  fileSuggestion,
  sendReply,
  startAllSuggestions,
  startSuggestion,
} from "@/lib/sessions/suggestions"
import { issuesRouter } from "@/lib/trpc/issues"
import {
  assertRunnerVocabulary,
  effectiveCli,
  isRunnerModel,
  resolveSessionDevice,
  sendSession,
  withTriggerIds,
} from "@/lib/sessions/send"
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

const suggestionRef = z.object({ messageId: uuid, itemId: z.string().min(1).max(64) })

/** The cards a DEVICE may post (the server owns event/session cards). */
const DEVICE_CARD_KINDS = new Set([`question`, `plan`, `subagent`, `picture`])

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
  fresh: z.boolean().optional(),
  feedback: z.string().max(MESSAGE_BODY_MAX).optional(),
  // EXP-1280: a plan's answers by question id, a question's picked labels;
  // the inline surface's data model at answer time.
  answers: z
    .union([
      planCardAnswersSchema,
      z.array(z.string().max(PLAN_CARD_LABEL_MAX)).max(QUESTION_CARD_OPTIONS_MAX),
    ])
    .optional(),
  surfaceData: z.record(z.string(), z.unknown()).optional(),
})

// COMPAT (EXP-1280): delete once CLIENT_MIN_VERSION_IOS > 0.14.62 AND
// CLIENT_MIN_VERSION_ANDROID > 0.14.64 AND CLIENT_MIN_VERSION_DESKTOP/_CLI >
// 0.14.72 (the last clients shipped before the rich plan card's questions).
// Their Approve sends a decision and no `answers`: on a plan with questions
// that is a plain approval (the agent reads the plan as approved, the
// questions unanswered), never the `missing` refusal a current client's
// form would get for leaving one blank.
const PLAN_ANSWERS_LEGACY_CLIENTS = {
  ios: `0.14.62`,
  android: `0.14.64`,
  desktop: `0.14.72`,
  cli: `0.14.72`,
} as const

/** The card message + its session (steer-gated) + the answered card. */
async function loadCardForAnswer(
  database: Database,
  userId: string,
  input: z.infer<typeof cardAnswerInput>,
  request?: Pick<Request, `headers`> | null
) {
  const [message] = await database.select().from(messages).where(eq(messages.id, input.messageId)).limit(1)
  if (!message) throw new TRPCError({ code: `NOT_FOUND`, message: `Message not found` })
  const session = await loadSessionForUser(userId, message.sessionId)
  await assertCanSteer(userId, session)
  const legacyPlanApproval =
    !!request && clientAtOrBelow(request, PLAN_ANSWERS_LEGACY_CLIENTS)
  const answered = answeredCard(message.card as MessageCard | null, {
    ...input,
    legacyPlanApproval,
  })
  if (!answered.ok) throw new TRPCError({ code: answered.code, message: answered.message })
  return { message, session, card: answered.card }
}

/** Stamp `card` + `answered_at` while still unanswered; needs-you clears
 *  once nothing else waits on a person. `claimed` = this call stamped it.
 *  EXP-1280: an approved plan takes the session out of plan mode (the
 *  `pr_open` gate opens) in the same transaction. */
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
    if (card.kind === `plan` && card.decision === `approved`) {
      await tx
        .update(sessions)
        .set({ planMode: false, updatedAt: now })
        .where(and(eq(sessions.id, sessionId), eq(sessions.planMode, true)))
    }
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

// COMPAT (EXP-1334): delete once CLIENT_MIN_VERSION_IOS > 0.14.61 AND
// CLIENT_MIN_VERSION_ANDROID > 0.14.63 AND CLIENT_MIN_VERSION_DESKTOP/_CLI >
// 0.14.71 (the last clients shipped before EXP-1334's link message). Those
// clients call `linkIssues` as a plain "Link an issue": they know nothing of
// the `Link #IDENT` message, and on an ENDED root that message would make
// `sendSession` spawn a continuation child (a queued session their UI never
// shows, which never runs when the host is offline). For them the link on an
// ended thread stays silent; current clients, the web app included, keep the
// continuation. The link itself always lands.
const LINK_MESSAGE_LEGACY_CLIENTS = {
  ios: `0.14.61`,
  android: `0.14.63`,
  desktop: `0.14.71`,
  cli: `0.14.71`,
} as const

async function legacyLinkOnEndedRoot(
  ctx: { db: typeof db; request: Pick<Request, `headers`> },
  session: Pick<Session, `id` | `rootId` | `status`>
): Promise<boolean> {
  if (!clientAtOrBelow(ctx.request, LINK_MESSAGE_LEGACY_CLIENTS)) return false
  if (session.id === session.rootId) return session.status === `ended`
  const [root] = await ctx.db
    .select({ status: sessions.status })
    .from(sessions)
    .where(eq(sessions.id, session.rootId))
    .limit(1)
  return root?.status === `ended`
}

export const sessionsRouter = router({
  // ── people (and agents over MCP) ──────────────────────────────────────────

  /** THE entrypoint. → { sessionId, messageId, txid }. */
  send: authedProcedure
    .input(sessionsSendInputSchema)
    .mutation(({ ctx, input }): Promise<SessionsSendResult & { txid: number }> =>
      sendSession({
        userId: ctx.session.user.id,
        input,
        // EXP-1323: a session's own key stamps its messages (validated in
        // `sendSession`: only the session's starter or host counts). Over
        // tRPC the header only STAMPS `author_session_id` on a person's row;
        // the agent-authored session-input path is MCP's alone (`viaMcp`),
        // so a person's text can never turn into "from session A".
        callerSessionId: ctx.request ? parseMcpSessionHeader(ctx.request) : null,
        viaMcp: ctx.viaMcp === true,
      })
    ),

  /** EXP-1332: the caller read this session (a thread = its root's row).
   *  Any session the caller may see; upserts `read_at = now`. */
  markRead: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .mutation(async ({ ctx, input }): Promise<{ txid: number }> => {
      const userId = ctx.session.user.id
      const session = await loadSessionForUser(userId, input.sessionId)
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const now = new Date()
        await tx
          .insert(sessionReads)
          .values({ userId, sessionId: session.id, readAt: now })
          .onConflictDoUpdate({
            target: [sessionReads.userId, sessionReads.sessionId],
            set: { readAt: now, updatedAt: now },
          })
        return { txid }
      })
    }),

  // ── EXP-1328: the suggestions card (posted by MCP `sessions_suggest`) ──

  /** Start = a child of the card's session (`where: child`, the item's text
   *  as its request, its title on the child); Do it here = a message to the
   *  card's session. Stamps the item in the same transaction. */
  startSuggestion: authedProcedure
    .input(
      suggestionRef.extend({
        where: z.enum([`child`, `here`]),
        deviceId: z.string().min(1).max(128).optional(),
        account: z.string().min(1).max(64).optional(),
        model: z.string().max(64).optional(),
      })
    )
    .mutation(({ ctx, input }) => startSuggestion(ctx.session.user.id, input)),

  /** Start all = a child per open item. */
  startAllSuggestions: authedProcedure
    .input(z.object({ messageId: uuid }))
    .mutation(({ ctx, input }) => startAllSuggestions(ctx.session.user.id, input.messageId)),

  dismissSuggestion: authedProcedure
    .input(suggestionRef)
    .mutation(({ ctx, input }) => dismissSuggestion(ctx.session.user.id, input)),

  /** File as issue: on the thread's board, linked to the thread. */
  fileSuggestion: authedProcedure
    .input(suggestionRef)
    .mutation(({ ctx, input }) =>
      fileSuggestion(ctx.session.user.id, input, async (args) => {
        const { issue } = await issuesRouter.createCaller(ctx).create(args)
        return { issueId: issue.id }
      })
    ),

  // ── EXP-1211: quick replies (the `replies` card above the composer) ──

  /** A tap: the reply's text as the next message to the card's session. */
  sendReply: authedProcedure
    .input(suggestionRef)
    .mutation(({ ctx, input }) => sendReply(ctx.session.user.id, input)),

  dismissReplies: authedProcedure
    .input(z.object({ messageId: uuid }))
    .mutation(({ ctx, input }) => dismissReplies(ctx.session.user.id, input.messageId)),

  /** EXP-1334: change a live session's launch options mid-session (plan
   *  mode, effort, ultracode, computer use, MCP picks). Same rule as
   *  messaging it; an ended session refuses. The host device applies them:
   *  claude's plan mode at once, the rest at the next turn. */
  configure: authedProcedure
    .input(sessionsConfigureInputSchema)
    .mutation(({ ctx, input }) => configureSession(ctx.session.user.id, input)),

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

  /** Link issues to a thread (always lands on the ROOT; the header "+"). */
  linkIssues: authedProcedure
    .input(z.object({ sessionId: uuid, issueIds: z.array(uuid).min(1).max(30) }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const session = await loadSessionForUser(userId, input.sessionId)
      await assertCanSteer(userId, session)
      const { txid, identifiers } = await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const { linked } = await linkIssues(tx, {
          rootId: session.rootId,
          issueIds: input.issueIds,
          source: `user`,
          addedBy: userId,
          addedBySessionId: null,
        })
        return { txid, identifiers: await issueIdentifiers(tx, linked) }
      })
      // EXP-1334: a manual link is the person asking for the work: `Link
      // #A #B` on the ROOT wakes the dispatcher, which routes it (MCP and
      // start links stay quiet). The link stands even if the message fails.
      const text = manualLinkText(identifiers)
      if (text && !(await legacyLinkOnEndedRoot(ctx, session))) {
        try {
          await sendSession({ userId, input: { sessionId: session.rootId, text } })
        } catch (err) {
          console.warn(`[sessions.linkIssues] the link message to ${session.rootId} failed`, err)
        }
      }
      return { txid }
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
      const { message, session, card } = await loadCardForAnswer(
        ctx.db,
        ctx.session.user.id,
        input,
        ctx.request
      )
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
   *  the device's pickup never feeds it again. Ask dock F1: a FRESH approval
   *  the device could not honour (it ran on in place) comes back as a plain
   *  approval and downgrades the stored card's `fresh` — never the reverse. */
  resolveCard: authedProcedure
    .input(cardAnswerInput)
    .mutation(async ({ ctx, input }): Promise<{ ok: true; txid?: number }> => {
      const { message, session, card } = await loadCardForAnswer(
        ctx.db,
        ctx.session.user.id,
        input,
        ctx.request
      )
      if (message.answeredAt) {
        const stored = message.card as MessageCard | null
        const downgrade =
          stored?.kind === `plan` &&
          stored.fresh === true &&
          input.decision === `approved` &&
          input.fresh !== true
        if (!downgrade) return { ok: true }
        const txid = await ctx.db.transaction(async (tx) => {
          const txid = await generateTxId(tx)
          await tx
            .update(messages)
            .set({ card: { ...stored, fresh: false }, updatedAt: new Date() })
            .where(eq(messages.id, message.id))
          return txid
        })
        return { ok: true, txid }
      }
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
      // The model comes from the agent's CLI vocabulary like a send's; a
      // blank (an old client's "CLI default") = unset, never stored.
      const model = input.model === undefined ? undefined : input.model || null
      if (model) assertRunnerVocabulary({ cli: agent.cli ?? undefined, model })
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
            ...(model !== undefined ? { model } : {}),
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
      return {
        ...(await composeSessionBrief(session.id)),
        // EXP-1252: a moved session's package to restore (`GET
        // /api/sessions/{id}/handoff`), null otherwise.
        handoff: briefHandoff(session),
      }
    }),

  /** EXP-1334: the session's server-only MCP pick (sorted ids) — for
   *  whoever may steer it (the composer's MCP submenu; the host device
   *  compares it with what its run launched with between turns). The ids
   *  stay off the synced shape. */
  mcpPicks: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .query(async ({ ctx, input }): Promise<{ mcpServerIds: string[] }> => {
      const session = await loadSessionForUser(ctx.session.user.id, input.sessionId)
      await assertCanSteer(ctx.session.user.id, session)
      return { mcpServerIds: mcpPicks(session) }
    }),

  /** queued → running: the device took it (idempotent for its own device). */
  claim: authedProcedure
    .input(
      z.object({
        sessionId: uuid,
        // EXP-1252: the claiming device; another device's claim is refused
        // (the row moved away from it).
        deviceId: z.string().min(1).max(128).optional(),
        branch: z.string().max(255).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const session = await loadSessionForUser(ctx.session.user.id, input.sessionId)
      assertDeviceHost(ctx.session.user.id, session)
      // CONFLICT (not FORBIDDEN): the daemon's `claim_lost` treats 404/409
      // as "moved away", anything else as a start failure it posts about.
      if (input.deviceId !== undefined && input.deviceId !== session.deviceId) {
        throw new TRPCError({
          code: `CONFLICT`,
          message: `This session runs on another device`,
        })
      }
      if (session.status === `ended`) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `This session has ended` })
      }
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const now = new Date()
        // Guarded on the device the row was read on: a direct re-target
        // (`sessions.send({deviceId})` on a queued row) committing between
        // the read and this write would otherwise flip a row now queued on
        // ANOTHER device to running here — two engines on one session. A
        // 0.14.72 daemon sends no `deviceId`: the snapshot's device stands
        // in, which the same guard still holds to. CONFLICT = `claim_lost`.
        const claimed = await tx
          .update(sessions)
          .set({
            ...(session.status === `queued` ? { status: `running` as const } : {}),
            startedAt: session.startedAt ?? now,
            ...(input.branch ? { branch: input.branch } : {}),
            updatedAt: now,
          })
          .where(
            and(
              eq(sessions.id, session.id),
              eq(sessions.deviceId, input.deviceId ?? session.deviceId),
              ne(sessions.status, `ended`)
            )
          )
          .returning({ id: sessions.id })
        if (claimed.length === 0) {
          throw new TRPCError({
            code: `CONFLICT`,
            message: `This session runs on another device`,
          })
        }
        // The brief's Request IS the session's first person message: it is
        // delivered by the claim, so the device never feeds it twice. Only a
        // FIRST start: a moved session re-queues with its messages pending.
        if (session.status === `queued` && !session.startedAt) {
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
        // EXP-1252: the reporting device; another device's report is a
        // no-op (a stale host after a move).
        deviceId: z.string().min(1).max(128).optional(),
        status: sessionStatusSchema.exclude([`queued`]).optional(),
        busy: z.boolean().optional(),
        needsInput: z.boolean().optional(),
        // `handled` = the device rotates past the wall itself (EXP-1005): no
        // push. Never stored.
        blocked: codingSessionBlockedSchema
          .extend({ handled: z.boolean().nullish() })
          .nullable()
          .optional(),
        branch: z.string().max(255).optional(),
        title: z.string().max(255).optional(),
        // EXP-849: the login the run ACTUALLY runs on (a start pick or a
        // rotation may move it off the row's), so the row says it and a
        // person's later pick of the old one is a real switch.
        account: z.string().min(1).max(64).optional(),
        // EXP-1331: the live status line (throttled by the device; null
        // clears it). Trimmed and capped at 80 here.
        caption: z.string().max(1000).nullable().optional(),
        // EXP-1334: the agent's OWN plan-mode move (claude leaving plan mode
        // once a plan is approved, or entering it): the row follows the
        // run, so every client's toggle says the mode it is really in. A
        // dispatcher never plans: ignored there.
        planMode: z.boolean().optional(),
        // EXP-1334: the agent's model as the run reports it (a `/model`
        // switch mid-session), so a resume launches on it. Outside the
        // agent's vocabulary (a full model id) = ignored, never refused.
        model: z.string().min(1).max(64).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const session = await loadSessionForUser(ctx.session.user.id, input.sessionId)
      assertDeviceHost(ctx.session.user.id, session)
      if (
        session.status === `ended` ||
        (input.deviceId !== undefined && input.deviceId !== session.deviceId)
      ) {
        // An ended row stays ended; the device's late reports are no-ops,
        // and so are a device's the session moved away from.
        return ctx.db.transaction(async (tx) => ({ txid: await generateTxId(tx) }))
      }
      // An agent never ends: its process going away leaves it idle.
      const status =
        input.status === `ended` && isPersistent(session) ? `idle` : input.status
      const ending = status === `ended`
      const now = new Date()
      const { handled = false, ...wall } = input.blocked ?? {}
      const blocked = (input.blocked ? wall : input.blocked) as CodingSessionBlocked | null | undefined
      const newlyBlocked = !!blocked && !session.blocked && !handled
      const model =
        input.model !== undefined &&
        !session.dispatch &&
        isRunnerModel(session.cli ?? (await effectiveCli(session)), input.model)
          ? input.model
          : undefined
      const turnEnded =
        (input.busy === false && session.busy) || status === `idle` || ending
      const txid = await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        // Guarded on the row's live status: a device end racing an explicit
        // End must not end it twice (= two report cards on the parent).
        const updated = await tx
          .update(sessions)
          .set({
            ...(status ? { status } : {}),
            ...(ending
              ? { endedAt: now, busy: false, needsInput: false }
              : {
                  // #FEED-85: idle with no explicit busy = the turn is over.
                  ...(input.busy !== undefined
                    ? { busy: input.busy }
                    : status === `idle`
                      ? { busy: false }
                      : {}),
                  ...(input.needsInput !== undefined ? { needsInput: input.needsInput } : {}),
                }),
            ...(input.blocked !== undefined ? { blocked: blocked ?? null } : {}),
            ...(input.branch ? { branch: input.branch } : {}),
            ...(input.account ? { account: input.account } : {}),
            ...(input.planMode !== undefined && !session.dispatch ? { planMode: input.planMode } : {}),
            ...(model ? { model } : {}),
            // The CLI's auto-name only fills a blank title (a person's or
            // the agent's own title wins).
            ...(input.title?.trim() && !session.title ? { title: input.title.trim() } : {}),
            ...(turnEnded ? { lastTurnAt: now } : {}),
            // The caption is the device's; a turn's end clears a stale one
            // unless the device says otherwise (an aside keeps its answer).
            ...(input.caption !== undefined
              ? { caption: normalizeCaption(input.caption) }
              : turnEnded && !session.aside && !session.moveToDeviceId && session.caption
                ? { caption: null }
                : {}),
            updatedAt: now,
          })
          .where(and(eq(sessions.id, session.id), ne(sessions.status, `ended`)))
          .returning({ id: sessions.id })
        // EXP-1329: the agent process went away = the child ended: report
        // to its parent like an explicit End (once: only when THIS report
        // flipped the row).
        if (ending && updated.length > 0) await postReportToParent(tx, session)
        // EXP-1350: a work child's turn ended (or it did): its follow-up's
        // issues may go back to In Review.
        if (updated.length > 0 && turnEnded && session.parentId !== null && !session.dispatch) {
          await settleFollowUpIssuesInTx(tx, session, [session.id], session.userId)
        }
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
      // A question card's `options` are labels; their descriptions ride the
      // parallel `optionDescriptions` sidecar.
      if (
        card?.kind === `question` &&
        card.options !== undefined &&
        !z.array(questionCardOptionSchema).max(QUESTION_CARD_OPTIONS_MAX).safeParse(card.options).success
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

  /** EXP-1252: the SOURCE host's outcome of a pending move. `moved` (the
   *  package is uploaded) re-stamps the row onto the target, queued;
   *  `failed` clears the move and the session stays here. */
  handoff: authedProcedure
    .input(
      z.object({
        sessionId: uuid,
        deviceId: z.string().min(1).max(128),
        outcome: z.enum([`moved`, `failed`]),
        reason: z.string().max(2000).optional(),
      })
    )
    .mutation(({ ctx, input }) =>
      completeHandoff({ userId: ctx.session.user.id, ...input })
    ),

  /** EXP-1252: the TARGET restored the package; the server drops it. */
  handoffDone: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .mutation(({ ctx, input }) =>
      finishHandoff({ userId: ctx.session.user.id, sessionId: input.sessionId })
    ),

  /** The device delivered these pending inputs (person messages, another
   *  session's question or report) into the agent. EXP-1280: also the
   *  session's OWN answered plan/question card (`exponential_plan_propose`,
   *  `exponential_sessions_ask`) once its answer text was delivered. */
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
              // EXP-1323: a person's message or another session's input.
              or(pendingInput(), answeredOwnCard()),
              isNull(messages.consumedAt)
            )
          )
        return { txid }
      })
    }),
})
