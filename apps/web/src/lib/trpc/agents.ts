import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, isNull, or, sql } from "drizzle-orm"
import {
  AGENT_INSTRUCTIONS_MAX_BYTES,
  SKILLS_PER_SESSION_MAX,
  agentFacetsSchema,
  agentTriggersSchema,
  boardIconSchema,
  hexColorSchema,
  type AgentTrigger,
  type agentTriggersSchema as AgentTriggersSchema,
} from "@exp/db-schema/domain"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { assertSkillsInTeam } from "@/lib/skills-guard"
import { db } from "@/db/connection"
import { appTriggerIssues, mergeTriggerWrite, TRIGGER_ISSUE_MESSAGES } from "@/lib/agents/app-triggers"
import {
  agents,
  apps,
  attachments,
  conversations,
  devices,
  messages,
  sessionIssues,
  sessions,
  teams,
  type Agent,
  type DeviceAgentAccounts,
} from "@/db/schema"
import { assertTeamMember, assertTeamOwner, getTeamMember } from "@/lib/auth/membership"
import { conversationInTx, loadAgentForUser } from "@/lib/agents/conversation"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"
import { hidePrivateLinesInTx } from "@/lib/sessions/private-lines"
import { dropNonReaderKeysInTx } from "@/lib/sessions/tree-keys"
import { relayKillSessionsBestEffort } from "@/lib/sessions/lifecycle"
import {
  assertMcpServersInTeam,
  assertRunnerVocabulary,
  resolveSessionDevice,
  resolveSessionDeviceInTx,
} from "@/lib/sessions/send"
import { AGENT_MCP_ROW, ASSISTANT_COPY, COORDINATOR_COPY, LEAD_COPY } from "@exp/domain-contract/agent-settings"
import { assertAssistantTriggers, ensureAssistant, syncVoiceMirror } from "@/lib/agents/assistant"

// NEW-2: team AGENTS = definition rows (Settings → Agents; owner writes,
// members read the synced `agents` shape). An agent runs as conversation
// sessions (`lib/agents/conversation.ts`); the server fires its triggers
// (`lib/agents/triggers.ts`). Message one with `sessions.send({agentId})`.

const uuid = z.string().uuid()
const agentSkillIdsSchema = z.array(uuid).max(SKILLS_PER_SESSION_MAX)
// #EXP-1793 (agent-settings.json `mcpRow.max`): the send's own cap.
export const agentMcpServerIdsSchema = z.array(uuid).max(AGENT_MCP_ROW.max)

/** Agent triggers as stored: every trigger carries an id (minted here). */
export function withTriggerIds(triggers: z.infer<typeof AgentTriggersSchema>): AgentTrigger[] {
  return mergeTriggerWrite([], triggers)
}

/** #EXP-1829: the live app an agent belongs to (its HEAD package), if any. */
async function appOfAgent(agentId: string): Promise<{ id: string; createdBy: string | null; head: unknown } | null> {
  const [row] = await db
    .select({ id: apps.id, createdBy: apps.createdBy, package: apps.package, draftPackage: apps.draftPackage })
    .from(apps)
    .where(and(eq(apps.agentId, agentId), isNull(apps.deletedAt)))
    .limit(1)
  return row ? { id: row.id, createdBy: row.createdBy, head: row.draftPackage ?? row.package ?? {} } : null
}

/** #EXP-1829: a trigger list the agent may hold (app kinds and `run` on an
 *  app's agent, checked against the app's head package). */
export function assertTriggersFit(triggers: readonly AgentTrigger[], app: { head: unknown } | null): void {
  const issues = appTriggerIssues(triggers, app ? app.head : null)
  if (issues.length === 0) return
  const words = issues.slice(0, 5).map((issue) => `triggers[${issue.index}]: ${TRIGGER_ISSUE_MESSAGES[issue.code] ?? `invalid (${issue.code})`}`)
  throw new TRPCError({ code: `BAD_REQUEST`, message: words.join(`; `) })
}

async function loadOwnedAgent(userId: string, id: string): Promise<Agent> {
  const agent = await loadAgentForUser(userId, id)
  // S-3: an Assistant is its member's alone (agent-settings.json
  // `assistant`); a team agent is the team owners'.
  if (agent.role === `assistant`) {
    if (agent.ownerId !== userId) throw new TRPCError({ code: `NOT_FOUND`, message: `Agent not found` })
    return agent
  }
  // #EXP-1829: an app's agent is also its app's creator's to manage (the
  // app's triggers live on it).
  const app = await appOfAgent(agent.id)
  if (app?.createdBy && app.createdBy === userId) return agent
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

/** S-3: the login an agent runs on, checked against its device's report.
 *  A named profile the device does not have falls back to the device's
 *  ACTIVE profile (else none = the device's own default); a device that
 *  reported no profiles for the agent is unknowable and keeps the name. */
export function reconcileAgentAccount(
  account: string | null,
  cli: string | null,
  accounts: DeviceAgentAccounts | null | undefined
): string | null {
  const profiles = accounts?.[cli ?? `claude`]?.profiles ?? []
  if (!account || profiles.length === 0) return account
  if (profiles.some((profile) => profile.id === account)) return account
  return profiles.find((profile) => profile.active)?.id ?? null
}

/** The device's per-agent logins as its owner's row reports them; null when
 *  the caller may no longer see the machine (nothing to check against). */
async function deviceAgentAccounts(
  tx: Parameters<typeof resolveSessionDeviceInTx>[0],
  ownerId: string | null,
  deviceId: string
): Promise<DeviceAgentAccounts | null> {
  if (!ownerId) return null
  const [row] = await tx
    .select({ agentAccounts: devices.agentAccounts })
    .from(devices)
    .where(and(eq(devices.deviceId, deviceId), eq(devices.userId, ownerId)))
    .limit(1)
  return row?.agentAccounts ?? null
}

export const agentsRouter = router({
  /** Member read: the team's agents (MCP `agents_list`; clients sync the
   *  shape). S-3: the caller's own Assistant + the team agents, never
   *  another member's Assistant (`visibleAgent`: not theirs to pick). */
  list: authedProcedure.input(z.object({ teamId: uuid })).query(async ({ ctx, input }) => {
    const userId = ctx.session.user.id
    await assertTeamMember(userId, input.teamId)
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
        role: agents.role,
        sortOrder: agents.sortOrder,
        instructionsUpdatedAt: agents.instructionsUpdatedAt,
        createdAt: agents.createdAt,
        updatedAt: agents.updatedAt,
      })
      .from(agents)
      .where(
        and(eq(agents.teamId, input.teamId), or(eq(agents.role, `agent`), eq(agents.ownerId, userId)))
      )
      .orderBy(asc(agents.sortOrder), asc(agents.name))
    // #EXP-1735 (agent-settings.json `lead`): the team's lead Assistant is
    // named with `lead: true` (another member's is listed by id, name and
    // owner only) so every member's Assistant knows whom it reports to.
    const lead = await teamLead(ctx.db, input.teamId)
    const listed = rows.map((row) => ({ ...row, lead: row.id === lead?.agentId }))
    if (lead && !rows.some((row) => row.id === lead.agentId)) {
      listed.push({ id: lead.agentId, teamId: input.teamId, name: lead.name, ownerId: lead.userId, role: `assistant`, lead: true } as (typeof listed)[number])
    }
    return { agents: listed }
  }),

  /** #EXP-1735 (agent-settings.json `lead`): the team's lead Assistant and
   *  its member (the Team settings row's value), or null. */
  lead: authedProcedure.input(z.object({ teamId: uuid })).query(async ({ ctx, input }) => {
    await assertTeamMember(ctx.session.user.id, input.teamId)
    const lead = await teamLead(ctx.db, input.teamId)
    return { lead: lead ? { agentId: lead.agentId, userId: lead.userId } : null }
  }),

  /** #EXP-1735 (agent-settings.json `lead`): the team's owners make one
   *  member's Assistant the team's lead (created on first use; null =
   *  none) and open its conversation, where the other members' Assistants'
   *  asks and reports land (bus.ts `leadOf`). */
  setLeadAssistant: authedProcedure
    .input(z.object({ teamId: uuid, userId: z.string().min(1).nullable() }))
    .mutation(async ({ ctx, input }) => {
      try {
        await assertTeamOwner(ctx.session.user.id, input.teamId)
      } catch {
        throw new TRPCError({ code: `FORBIDDEN`, message: LEAD_COPY.refusedOwner })
      }
      if (input.userId && !(await getTeamMember(input.userId, input.teamId))) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: LEAD_COPY.refusedMember })
      }
      const lead = input.userId ? await ensureAssistant(input.userId, input.teamId) : null
      if (input.userId && !lead) throw new TRPCError({ code: `BAD_REQUEST`, message: LEAD_COPY.noAssistant })
      let host: { ownerId: string; deviceId: string } | null = null
      const result = await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        await tx
          .update(teams)
          .set({ leadAssistantId: lead?.id ?? null, updatedAt: new Date() })
          .where(eq(teams.id, input.teamId))
        // The lead replaces the coordinator (#EXP-1730 phase B drops the
        // role): a lead pick sends the team's coordinator back to a team
        // agent in the same write, so old clients' Coordinator row reads None.
        if (lead) {
          await tx
            .update(agents)
            .set({ role: `agent`, updatedAt: new Date() })
            .where(and(eq(agents.teamId, input.teamId), eq(agents.role, `coordinator`)))
        }
        if (lead) {
          // Its conversation is where the reports land: open it now (best
          // effort: a device that cannot run it leaves it for first use).
          try {
            host = (await tx.transaction(async (inner) => conversationInTx(inner, lead, lead.ownerId))).created
          } catch (error) {
            console.warn(`[agents.setLeadAssistant] conversation of ${lead.id} not opened:`, error)
          }
        }
        return { txid }
      })
      const config = getSteerRelayConfig()
      const opened = host as { ownerId: string; deviceId: string } | null
      if (config && opened) void relayPostNudge(config, opened.ownerId, opened.deviceId)
      return result
    }),

  /** S-3: the caller's Assistant in the team, created on first use (null
   *  while they have no device to run it on). */
  ensureAssistant: authedProcedure.input(z.object({ teamId: uuid })).mutation(async ({ ctx, input }) => {
    const agent = await ensureAssistant(ctx.session.user.id, input.teamId)
    if (!agent) return { agent: null }
    const { triggerState: _state, ...rest } = agent
    return { agent: rest }
  }),

  /** #EXP-1717 (agent-settings.json `coordinator`): the team's owners make
   *  one team agent its coordinator (null = none); the previous one goes
   *  back to a team agent in the same write. */
  setCoordinator: authedProcedure
    .input(z.object({ teamId: uuid, agentId: uuid.nullable() }))
    .mutation(async ({ ctx, input }) => {
      try {
        await assertTeamOwner(ctx.session.user.id, input.teamId)
      } catch {
        throw new TRPCError({ code: `FORBIDDEN`, message: COORDINATOR_COPY.refusedOwner })
      }
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        // One coordinator per team: concurrent picks serialize on the team row
        // (no partial unique index: an enum value added by a migration can't
        // be used in the same migration transaction).
        await tx.select({ id: teams.id }).from(teams).where(eq(teams.id, input.teamId)).for(`update`)
        if (input.agentId) {
          const [agent] = await tx
            .select({ teamId: agents.teamId, role: agents.role })
            .from(agents)
            .where(eq(agents.id, input.agentId))
            .limit(1)
          if (!agent || agent.teamId !== input.teamId || agent.role === `assistant`) {
            throw new TRPCError({ code: `BAD_REQUEST`, message: COORDINATOR_COPY.refusedAgent })
          }
        }
        await tx
          .update(agents)
          .set({ role: `agent`, updatedAt: new Date() })
          .where(and(eq(agents.teamId, input.teamId), eq(agents.role, `coordinator`)))
        if (input.agentId) {
          await tx
            .update(agents)
            .set({ role: `coordinator`, updatedAt: new Date() })
            .where(eq(agents.id, input.agentId))
        }
        return { txid }
      })
    }),

  /** Member read: the row with its server-only instructions. */
  get: authedProcedure.input(z.object({ id: uuid })).query(async ({ ctx, input }) => {
    const agent = await loadAgentForUser(ctx.session.user.id, input.id)
    const { triggerState: _state, ...rest } = agent
    return { agent: rest }
  }),

  create: authedProcedure
    .input(
      agentFacetsSchema.extend({
        teamId: uuid,
        skillIds: agentSkillIdsSchema.optional(),
        mcpServerIds: agentMcpServerIdsSchema.optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      await assertTeamOwner(userId, input.teamId)
      const model = input.model || null
      assertRunnerVocabulary({ cli: input.cli, ...(model ? { model } : {}) })
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        // The agent runs on the owner's device (own, or shared with the
        // team), read under the machine's share lock (#EXP-1259).
        await resolveSessionDeviceInTx(tx, userId, input.teamId, input.deviceId)
        const skillIds = (await assertSkillsInTeam(tx, input.teamId, input.skillIds)) ?? []
        const mcpServerIds = (await assertMcpServersInTeam(tx, input.teamId, input.mcpServerIds)) ?? []
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
            triggers: (() => {
              const triggers = withTriggerIds(input.triggers)
              assertTriggersFit(triggers, null)
              return triggers
            })(),
            deviceId: input.deviceId,
            cli: input.cli ?? null,
            account: input.account ?? null,
            model,
            skillIds,
            mcpServerIds,
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
        // #EXP-1656: the skills every run of the agent starts with.
        skillIds: agentSkillIdsSchema.optional(),
        // #EXP-1793: the team MCP servers every run of the agent starts with.
        mcpServerIds: agentMcpServerIdsSchema.optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const agent = await loadOwnedAgent(userId, input.id)
      if (agent.role === `assistant` && input.triggers !== undefined) assertAssistantTriggers(input.triggers)
      // #EXP-1829: a kept trigger keeps its prompt/run unless the write
      // names them; app kinds and runs only on an app's agent.
      const triggers = input.triggers === undefined ? undefined : mergeTriggerWrite(agent.triggers, input.triggers)
      if (triggers) assertTriggersFit(triggers, await appOfAgent(agent.id))
      // #EXP-1439: the Assistant's prompt is built in. An old client resends
      // the stored text unchanged on every save; only an EDIT is refused.
      if (agent.role === `assistant` && input.instructions !== undefined && input.instructions !== agent.instructions) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: ASSISTANT_COPY.promptFixed })
      }
      const cli = input.cli ?? agent.cli ?? undefined
      // The model comes from the CLI's vocabulary like a send's; a blank (an
      // old client's "CLI default") = unset, never stored.
      const model = input.model === undefined ? undefined : input.model || null
      if (model || input.cli) assertRunnerVocabulary({ cli, ...(model ?? agent.model ? { model: (model ?? agent.model)! } : {}) })
      // A new device re-stamps every conversation's visibility (its device's
      // share decides it) and hands them over: queued again, the new device
      // boots them, the old one drops rows that no longer name it.
      const result = await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const now = new Date()
        // #EXP-1259: the new device is read under its share lock.
        const moved =
          input.deviceId !== undefined && input.deviceId !== agent.deviceId
            ? await resolveSessionDeviceInTx(tx, userId, agent.teamId, input.deviceId)
            : null
        // S-3: the login is re-checked on every save against the device the
        // agent runs on, and the conversations take the SAME value, so the
        // definition and its sessions never drift apart (a profile the
        // device lacks falls back to its active one).
        const deviceId = moved ? input.deviceId! : agent.deviceId
        const ownerId = moved
          ? moved.ownerId
          : await resolveSessionDevice(userId, agent.teamId, deviceId, tx).then(
              (device) => device.ownerId,
              () => null
            )
        const account = reconcileAgentAccount(
          input.account !== undefined ? input.account : agent.account,
          cli ?? null,
          await deviceAgentAccounts(tx, ownerId, deviceId)
        )
        // Only an explicit pick or a reconcile CHANGE writes the login: a
        // rename / toggle / reorder never undoes a device-reported rotation
        // of a conversation's account (EXP-849).
        const accountWrite = input.account !== undefined || account !== agent.account ? { account } : {}
        const skillIds =
          input.skillIds === undefined ? undefined : ((await assertSkillsInTeam(tx, agent.teamId, input.skillIds)) ?? [])
        const mcpServerIds =
          input.mcpServerIds === undefined
            ? undefined
            : ((await assertMcpServersInTeam(tx, agent.teamId, input.mcpServerIds)) ?? [])
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
            ...(triggers !== undefined ? { triggers } : {}),
            ...(input.cli !== undefined ? { cli: input.cli } : {}),
            ...accountWrite,
            ...(model !== undefined ? { model } : {}),
            ...(skillIds !== undefined ? { skillIds } : {}),
            ...(mcpServerIds !== undefined ? { mcpServerIds } : {}),
            ...(input.sortOrder !== undefined ? { sortOrder: input.sortOrder } : {}),
            ...(moved ? { deviceId: input.deviceId } : {}),
            updatedAt: now,
          })
          .where(eq(agents.id, agent.id))
          .returning()
        const ids = await conversationIds(tx, agent.id)
        if (ids.length > 0) {
          // The conversations follow the definition: the login + model they
          // resume on and (on a move) the device.
          await tx
            .update(sessions)
            .set({
              ...(input.cli !== undefined ? { cli: input.cli } : {}),
              ...accountWrite,
              ...(model !== undefined ? { model } : {}),
              // #EXP-1656: and the skills its next launch installs.
              ...(skillIds !== undefined ? { skillIds: skillIds.length > 0 ? skillIds : null } : {}),
              // #EXP-1793: and the MCP servers it resolves (re-read between turns).
              ...(mcpServerIds !== undefined ? { mcpServerIds: mcpServerIds.length > 0 ? mcpServerIds : null } : {}),
              ...(moved
                ? { deviceId: input.deviceId, shared: moved.shared, status: `queued` as const, busy: false, background: false }
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
            // #FEED-177: trees gone private keep only their readers' key wraps.
            if (!moved.shared) await dropNonReaderKeysInTx(tx, ids)
          }
        }
        const { triggerState: _state, ...rest } = updated!
        return { agent: rest, txid }
      })
      // S-3: the Assistant's device IS the member's voice device.
      if (agent.role === `assistant` && result.agent.deviceId !== agent.deviceId) {
        await syncVoiceMirror(ctx.db, agent.ownerId)
      }
      return result
    }),

  /** Deletes the definition and, by cascade, every conversation with its tree. */
  delete: authedProcedure.input(z.object({ id: uuid })).mutation(async ({ ctx, input }) => {
    const userId = ctx.session.user.id
    const agent = await loadOwnedAgent(userId, input.id)
    if (agent.role === `assistant`) throw new TRPCError({ code: `BAD_REQUEST`, message: ASSISTANT_COPY.deleteRefused })
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

/** #EXP-1735: the team's lead Assistant (`teams.lead_assistant_id`) while
 *  its member is still in the team, with that member's id and its name. */
async function teamLead(
  exec: { select: typeof import("@/db/connection").db.select },
  teamId: string
): Promise<{ agentId: string; userId: string; name: string } | null> {
  const [row] = await exec
    .select({ agentId: agents.id, userId: agents.ownerId, name: agents.name, role: agents.role })
    .from(teams)
    .innerJoin(agents, eq(agents.id, teams.leadAssistantId))
    .where(eq(teams.id, teamId))
    .limit(1)
  if (!row || row.role !== `assistant` || !(await getTeamMember(row.userId, teamId))) return null
  return { agentId: row.agentId, userId: row.userId, name: row.name }
}
