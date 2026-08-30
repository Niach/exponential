import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, inArray } from "drizzle-orm"
import {
  automationDeviceIdSchema,
  automationTriggerSchema,
  type AutomationTrigger,
} from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import {
  actions,
  automations,
  boards,
  devices,
  issueStatuses,
  labels,
  teamMembers,
} from "@/db/schema"
import { assertTeamMember, assertTeamOwner } from "@/lib/team-membership"
import { isBuiltinActionId } from "@/lib/builtin-actions"
import {
  AUTOMATION_REQUIRED_INPUTS_MESSAGE,
  hasRequiredInput,
} from "@/lib/trpc/actions"

// Automations (EXP-583, split out of actions.trigger from EXP-530): a schedule
// or issue-event trigger that runs ONE action on ONE device with its own
// agent/model/effort. Rows sync via the `automations` shape; this router is
// the write path (team-owner-only) plus a member-gated `list` for MCP.
// There is NO server scheduler — the bound device
// selects its enabled rows off Electric and self-starts the run
// (codingSessions.start with startedReason + automationId).

const codingAgentValues = contract.codingAgent.values as [string, ...string[]]
const agentModelValues: Record<string, readonly string[]> = {
  claude: contract.codingModel.values,
  codex: contract.codexModel.values,
  pi: contract.piModel.values,
}
const agentEffortValues: Record<string, readonly string[]> = {
  claude: contract.codingEffort.values,
  codex: contract.codexEffort.values,
  pi: contract.piThinking.values,
}

const wireColumns = {
  id: automations.id,
  teamId: automations.teamId,
  actionId: automations.actionId,
  deviceId: automations.deviceId,
  enabled: automations.enabled,
  trigger: automations.trigger,
  agent: automations.agent,
  model: automations.model,
  effort: automations.effort,
  sortOrder: automations.sortOrder,
  createdAt: automations.createdAt,
  updatedAt: automations.updatedAt,
}

// Agent/model/effort: NULL = the device's launch defaults. A model/effort is
// only meaningful against an agent, so both are validated against the
// (agent ?? claude) contract lists, mirroring steer.startSession.
const launchFieldsSchema = z.object({
  agent: z.enum(codingAgentValues).nullable().optional(),
  model: z.string().max(64).nullable().optional(),
  effort: z.string().max(32).nullable().optional(),
})

function assertLaunchFields(fields: {
  agent?: string | null
  model?: string | null
  effort?: string | null
}): void {
  const agent = fields.agent ?? `claude`
  const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })
  if (fields.model && !agentModelValues[agent]!.includes(fields.model)) {
    throw bad(`Unknown ${agent} model`)
  }
  if (fields.effort && !agentEffortValues[agent]!.includes(fields.effort)) {
    throw bad(`Unknown ${agent} effort`)
  }
}

async function loadAutomation(id: string) {
  const { db } = await import(`@/db/connection`)
  const [row] = await db
    .select(wireColumns)
    .from(automations)
    .where(eq(automations.id, id))
    .limit(1)
  if (!row) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Automation not found` })
  }
  return row
}

// The target must be a real custom action of the same team (builtins never
// automate) and, while the automation is enabled, declare no required input —
// automated runs fill none.
async function loadTargetAction(actionId: string, teamId: string) {
  const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })
  if (isBuiltinActionId(actionId)) throw bad(`Built-in actions can't be automated`)
  const { db } = await import(`@/db/connection`)
  const [action] = await db
    .select({ id: actions.id, teamId: actions.teamId, inputs: actions.inputs })
    .from(actions)
    .where(eq(actions.id, actionId))
    .limit(1)
  if (!action) throw bad(`Action not found`)
  if (action.teamId !== teamId) throw bad(`Action must belong to the team`)
  return action
}

function assertRunnable(inputs: unknown, enabled: boolean): void {
  if (enabled && hasRequiredInput(inputs)) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: AUTOMATION_REQUIRED_INPUTS_MESSAGE,
    })
  }
}

// The runner binding: `deviceId` is the steer TEXT id (devices.device_id) —
// unique per (userId, deviceId), so the same id can exist under several
// users: usable when any matching row is the caller's own, or is shared with
// THIS team by an owner who is still a member. The device must also advertise
// the `automations` cap — an ACTION cap (EXP-409), so its absence really means
// no agent is signed in on that machine, which is what the refusal says — and,
// when an agent is pinned, advertise that agent.
async function assertDeviceUsable(
  deviceId: string,
  teamId: string,
  callerUserId: string,
  agent: string | null | undefined
): Promise<void> {
  const { db } = await import(`@/db/connection`)
  const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })
  const rows = await db
    .select({
      userId: devices.userId,
      sharedTeamId: devices.sharedTeamId,
      caps: devices.caps,
      agents: devices.agents,
    })
    .from(devices)
    .where(eq(devices.deviceId, deviceId))
  let usableRows = rows.filter((row) => row.userId === callerUserId)
  if (usableRows.length === 0) {
    const shared = rows.filter((row) => row.sharedTeamId === teamId)
    if (shared.length > 0) {
      const members = await db
        .select({ userId: teamMembers.userId })
        .from(teamMembers)
        .where(
          and(
            eq(teamMembers.teamId, teamId),
            inArray(
              teamMembers.userId,
              shared.map((row) => row.userId)
            )
          )
        )
      const memberIds = new Set(members.map((m) => m.userId))
      usableRows = shared.filter((row) => memberIds.has(row.userId))
    }
  }
  if (usableRows.length === 0) {
    throw bad(`Automation device must be yours or shared with this team`)
  }
  if (!usableRows.some((row) => (row.caps ?? []).includes(`automations`))) {
    throw bad(
      `No agent is signed in on that machine — sign in on the device first`
    )
  }
  if (agent && !usableRows.some((row) => (row.agents ?? []).includes(agent))) {
    throw bad(`${agent} is not available on that device`)
  }
}

// Event filters must name THIS team's resources — reject, never clamp.
async function assertFiltersInTeam(
  trigger: AutomationTrigger,
  teamId: string
): Promise<void> {
  if (trigger.kind !== `event`) return
  const { db } = await import(`@/db/connection`)
  const assertIdsInTeam = async (
    ids: string[] | undefined,
    table: typeof boards | typeof labels | typeof issueStatuses,
    what: string
  ) => {
    if (!ids?.length) return
    const found = await db
      .select({ id: table.id })
      .from(table)
      .where(and(eq(table.teamId, teamId), inArray(table.id, ids)))
    if (found.length !== new Set(ids).size) {
      throw new TRPCError({
        code: `BAD_REQUEST`,
        message: `Automation ${what} must belong to the team`,
      })
    }
  }
  await assertIdsInTeam(trigger.filters?.boardIds, boards, `boards`)
  await assertIdsInTeam(trigger.filters?.labelIds, labels, `labels`)
  await assertIdsInTeam(trigger.filters?.toStatusIds, issueStatuses, `statuses`)
}

export const automationsRouter = router({
  list: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertTeamMember(ctx.session.user.id, input.teamId)
      const rows = await ctx.db
        .select(wireColumns)
        .from(automations)
        .where(eq(automations.teamId, input.teamId))
        .orderBy(asc(automations.sortOrder), asc(automations.createdAt))
      return { automations: rows }
    }),

  create: authedProcedure
    .input(
      z
        .object({
          teamId: z.string().uuid(),
          actionId: z.string().uuid(),
          deviceId: automationDeviceIdSchema,
          trigger: automationTriggerSchema,
          enabled: z.boolean().optional(),
        })
        .merge(launchFieldsSchema)
    )
    .mutation(async ({ ctx, input }) => {
      await assertTeamOwner(ctx.session.user.id, input.teamId)
      const enabled = input.enabled ?? true
      const action = await loadTargetAction(input.actionId, input.teamId)
      assertRunnable(action.inputs, enabled)
      assertLaunchFields(input)
      await assertDeviceUsable(
        input.deviceId,
        input.teamId,
        ctx.session.user.id,
        input.agent
      )
      await assertFiltersInTeam(input.trigger, input.teamId)

      return await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const [last] = await tx
          .select({ sortOrder: automations.sortOrder })
          .from(automations)
          .where(eq(automations.teamId, input.teamId))
          .orderBy(desc(automations.sortOrder))
          .limit(1)
        const [automation] = await tx
          .insert(automations)
          .values({
            teamId: input.teamId,
            actionId: input.actionId,
            deviceId: input.deviceId,
            enabled,
            trigger: input.trigger,
            agent: input.agent ?? null,
            model: input.model || null,
            effort: input.effort || null,
            sortOrder: (last?.sortOrder ?? 0) + 1,
          })
          .returning(wireColumns)
        return { automation: automation!, txid }
      })
    }),

  update: authedProcedure
    .input(
      z
        .object({
          id: z.string().uuid(),
          actionId: z.string().uuid().optional(),
          deviceId: automationDeviceIdSchema.optional(),
          trigger: automationTriggerSchema.optional(),
          enabled: z.boolean().optional(),
          sortOrder: z.number().finite().optional(),
        })
        .merge(launchFieldsSchema)
    )
    .mutation(async ({ ctx, input }) => {
      const existing = await loadAutomation(input.id)
      await assertTeamOwner(ctx.session.user.id, existing.teamId)

      const next = {
        actionId: input.actionId ?? existing.actionId,
        deviceId: input.deviceId ?? existing.deviceId,
        trigger: input.trigger ?? existing.trigger,
        enabled: input.enabled ?? existing.enabled,
        agent: input.agent === undefined ? existing.agent : input.agent,
        model: input.model === undefined ? existing.model : input.model || null,
        effort:
          input.effort === undefined ? existing.effort : input.effort || null,
      }
      // Post-update pairing: enabling re-checks the target's inputs.
      const action = await loadTargetAction(next.actionId, existing.teamId)
      assertRunnable(action.inputs, next.enabled)
      assertLaunchFields(next)
      // An UNCHANGED device binding is accepted as-is (co-owners can toggle an
      // automation bound to a teammate's private device they could never
      // re-mint) unless the agent pin changed, which needs the device's list.
      if (
        next.deviceId !== existing.deviceId ||
        (next.agent && next.agent !== existing.agent)
      ) {
        await assertDeviceUsable(
          next.deviceId,
          existing.teamId,
          ctx.session.user.id,
          next.agent
        )
      }
      if (input.trigger) await assertFiltersInTeam(input.trigger, existing.teamId)

      return await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const [automation] = await tx
          .update(automations)
          .set({
            ...next,
            ...(input.sortOrder !== undefined ? { sortOrder: input.sortOrder } : {}),
            updatedAt: new Date(),
          })
          .where(eq(automations.id, input.id))
          .returning(wireColumns)
        if (!automation) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Automation not found`,
          })
        }
        return { automation, txid }
      })
    }),

  delete: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const existing = await loadAutomation(input.id)
      await assertTeamOwner(ctx.session.user.id, existing.teamId)
      return await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        await tx.delete(automations).where(eq(automations.id, input.id))
        return { ok: true as const, txid }
      })
    }),
})
