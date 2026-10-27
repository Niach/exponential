import { z } from "zod"
import { and, asc, eq, inArray, isNull, or, sql } from "drizzle-orm"
import {
  AGENT_INSTRUCTIONS_MAX_BYTES,
  agentFacetsSchema,
  agentTriggersSchema,
  boardIconSchema,
  hexColorSchema,
  type AgentTrigger,
  type agentTriggersSchema as AgentTriggersSchema,
} from "@exp/db-schema/domain"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { agents, attachments, conversations, messages, sessionIssues, sessions, type Agent } from "@/db/schema"
import { assertTeamMember, assertTeamOwner } from "@/lib/auth/membership"
import { agentMirror, loadAgentForUser } from "@/lib/agents/conversation"
import { hidePrivateLinesInTx } from "@/lib/sessions/private-lines"
import { relayKillSessionsBestEffort } from "@/lib/sessions/lifecycle"
import { assertRunnerVocabulary, resolveSessionDevice } from "@/lib/sessions/send"

// NEW-2: team AGENTS = definition rows (Settings → Agents; owner writes,
// members read the synced `agents` shape). An agent runs as conversation
// sessions (`lib/agents/conversation.ts`); the server fires its triggers
// (`lib/agents/triggers.ts`). Message one with `sessions.send({agentId})`.

const uuid = z.string().uuid()

/** Agent triggers as stored: every trigger carries an id (minted here). */
export function withTriggerIds(triggers: z.infer<typeof AgentTriggersSchema>): AgentTrigger[] {
  return triggers.map(
    (trigger) => ({ ...trigger, id: trigger.id ?? crypto.randomUUID() }) as AgentTrigger
  )
}

async function loadOwnedAgent(userId: string, id: string): Promise<Agent> {
  const agent = await loadAgentForUser(userId, id)
  await assertTeamOwner(userId, agent.teamId)
  return agent
}

/** The agent's conversation sessions (one per member). */
async function conversationIds(tx: { select: typeof import("@/db/connection").db.select }, agentId: string) {
  const rows = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(and(eq(sessions.agentId, agentId), isNull(sessions.parentId)))
  return rows.map((row) => row.id)
}

export const agentsRouter = router({
  /** Member read: the team's agents (MCP `agents_list`; clients sync the shape). */
  list: authedProcedure.input(z.object({ teamId: uuid })).query(async ({ ctx, input }) => {
    await assertTeamMember(ctx.session.user.id, input.teamId)
    const rows = await ctx.db
      .select({
        id: agents.id,
        teamId: agents.teamId,
        name: agents.name,
        icon: agents.icon,
        color: agents.color,
        triggers: agents.triggers,
        deviceId: agents.deviceId,
        cli: agents.cli,
        account: agents.account,
        model: agents.model,
        ownerId: agents.ownerId,
        sortOrder: agents.sortOrder,
        instructionsUpdatedAt: agents.instructionsUpdatedAt,
        createdAt: agents.createdAt,
        updatedAt: agents.updatedAt,
      })
      .from(agents)
      .where(eq(agents.teamId, input.teamId))
      .orderBy(asc(agents.sortOrder), asc(agents.name))
    return { agents: rows }
  }),

  /** Member read: the row with its server-only instructions. */
  get: authedProcedure.input(z.object({ id: uuid })).query(async ({ ctx, input }) => {
    const agent = await loadAgentForUser(ctx.session.user.id, input.id)
    const { triggerState: _state, ...rest } = agent
    return { agent: rest }
  }),

  create: authedProcedure
    .input(agentFacetsSchema.extend({ teamId: uuid }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      await assertTeamOwner(userId, input.teamId)
      // The agent runs on the owner's device (own, or shared with the team).
      await resolveSessionDevice(userId, input.teamId, input.deviceId)
      const model = input.model || null
      assertRunnerVocabulary({ cli: input.cli, ...(model ? { model } : {}) })
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const [agent] = await tx
          .insert(agents)
          .values({
            teamId: input.teamId,
            name: input.name,
            icon: input.icon ?? null,
            color: input.color ?? null,
            instructions: input.instructions,
            // #EXP-1383 (chats.json `agentHint`): the text's birth.
            instructionsUpdatedAt: new Date(),
            triggers: withTriggerIds(input.triggers),
            deviceId: input.deviceId,
            cli: input.cli ?? null,
            account: input.account ?? null,
            model,
            ownerId: userId,
          })
          .returning()
        return { agent: agent!, txid }
      })
    }),

  update: authedProcedure
    .input(
      z.object({
        id: uuid,
        name: agentFacetsSchema.shape.name.optional(),
        icon: boardIconSchema.nullable().optional(),
        color: hexColorSchema.nullable().optional(),
        // NOT the facet schema's (its `.default('')` would wipe the
        // instructions on every update that leaves them out).
        instructions: z.string().max(AGENT_INSTRUCTIONS_MAX_BYTES).optional(),
        triggers: agentTriggersSchema.optional(),
        deviceId: z.string().min(1).max(128).optional(),
        cli: z.string().min(1).max(16).optional(),
        account: z.string().min(1).max(64).nullable().optional(),
        model: z.string().max(64).nullable().optional(),
        sortOrder: z.number().finite().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const agent = await loadOwnedAgent(userId, input.id)
      const cli = input.cli ?? agent.cli ?? undefined
      // The model comes from the CLI's vocabulary like a send's; a blank (an
      // old client's "CLI default") = unset, never stored.
      const model = input.model === undefined ? undefined : input.model || null
      if (model || input.cli) assertRunnerVocabulary({ cli, ...(model ?? agent.model ? { model: (model ?? agent.model)! } : {}) })
      // A new device re-stamps every conversation's visibility (its device's
      // share decides it) and hands them over: queued again, the new device
      // boots them, the old one drops rows that no longer name it.
      const moved =
        input.deviceId !== undefined && input.deviceId !== agent.deviceId
          ? await resolveSessionDevice(userId, agent.teamId, input.deviceId)
          : null
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const now = new Date()
        const [updated] = await tx
          .update(agents)
          .set({
            ...(input.name !== undefined ? { name: input.name } : {}),
            ...(input.icon !== undefined ? { icon: input.icon } : {}),
            ...(input.color !== undefined ? { color: input.color } : {}),
            // #EXP-1383: only a CHANGED text re-stamps (a rename, a trigger
            // toggle or a device pick never does).
            ...(input.instructions !== undefined && input.instructions !== agent.instructions
              ? { instructions: input.instructions, instructionsUpdatedAt: now }
              : {}),
            ...(input.triggers !== undefined ? { triggers: withTriggerIds(input.triggers) } : {}),
            ...(input.cli !== undefined ? { cli: input.cli } : {}),
            ...(input.account !== undefined ? { account: input.account } : {}),
            ...(model !== undefined ? { model } : {}),
            ...(input.sortOrder !== undefined ? { sortOrder: input.sortOrder } : {}),
            ...(moved ? { deviceId: input.deviceId } : {}),
            updatedAt: now,
          })
          .where(eq(agents.id, agent.id))
          .returning()
        const ids = await conversationIds(tx, agent.id)
        if (ids.length > 0) {
          // The conversations follow the definition: the legacy mirrors, the
          // login + model they resume on, and (on a move) the device.
          await tx
            .update(sessions)
            .set({
              ...agentMirror(updated!),
              ...(input.cli !== undefined ? { cli: input.cli } : {}),
              ...(input.account !== undefined ? { account: input.account } : {}),
              ...(model !== undefined ? { model } : {}),
              ...(moved
                ? { deviceId: input.deviceId, shared: moved.shared, status: `queued` as const, busy: false }
                : {}),
              updatedAt: now,
            })
            .where(inArray(sessions.id, ids))
          if (input.name !== undefined && input.name !== agent.name) {
            // The chats are titled after the agent: a rename follows them,
            // except where a member renamed the chat itself.
            await tx
              .update(sessions)
              .set({ title: input.name, updatedAt: now })
              .where(
                and(
                  inArray(sessions.id, ids),
                  or(sql`coalesce(trim(${sessions.title}), '') = ''`, eq(sessions.title, agent.name))
                )
              )
          }
          if (moved) {
            // The whole trees' visibility mirrors follow the new device.
            await tx
              .update(sessions)
              .set({ shared: moved.shared, updatedAt: now })
              .where(inArray(sessions.rootId, ids))
            await tx.update(messages).set({ shared: moved.shared }).where(inArray(messages.rootId, ids))
            await tx
              .update(conversations)
              .set({ shared: moved.shared })
              .where(
                inArray(
                  conversations.sessionId,
                  tx.select({ id: sessions.id }).from(sessions).where(inArray(sessions.rootId, ids))
                )
              )
            await tx
              .update(sessionIssues)
              .set({ shared: moved.shared })
              .where(
                inArray(
                  sessionIssues.sessionId,
                  tx.select({ id: sessions.id }).from(sessions).where(inArray(sessions.rootId, ids))
                )
              )
            // #EXP-1409: what a private chat's bus wrote in full while only
            // the owner read these trees goes neutral for their new readers.
            await hidePrivateLinesInTx(tx, ids)
          }
        }
        const { triggerState: _state, ...rest } = updated!
        return { agent: rest, txid }
      })
    }),

  /** Deletes the definition and, by cascade, every conversation with its tree. */
  delete: authedProcedure.input(z.object({ id: uuid })).mutation(async ({ ctx, input }) => {
    const userId = ctx.session.user.id
    const agent = await loadOwnedAgent(userId, input.id)
    const result = await ctx.db.transaction(async (tx) => {
      const txid = await generateTxId(tx)
      const roots = await conversationIds(tx, agent.id)
      const tree = roots.length
        ? await tx.select({ id: sessions.id }).from(sessions).where(inArray(sessions.rootId, roots))
        : []
      if (roots.length > 0) {
        // Message files go back to owner-less uploads (the orphan sweep
        // reclaims their blobs) before the cascade drops the messages.
        await tx
          .update(attachments)
          .set({ messageId: null })
          .where(
            inArray(
              attachments.messageId,
              tx.select({ id: messages.id }).from(messages).where(inArray(messages.rootId, roots))
            )
          )
      }
      await tx.delete(agents).where(eq(agents.id, agent.id))
      return { txid, ids: tree.map((row) => row.id) }
    })
    await relayKillSessionsBestEffort(result.ids)
    return { txid: result.txid }
  }),
})
