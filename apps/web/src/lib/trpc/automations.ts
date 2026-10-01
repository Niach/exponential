import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq } from "drizzle-orm"
import {
  automationDeviceIdSchema,
  legacyAutomationTriggerSchema,
  triggerWhenPart,
  type ActionTrigger,
  type ActionTriggerInput,
} from "@exp/db-schema/domain"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { actions, automations } from "@/db/schema"
import { assertTeamMember, assertTeamOwner } from "@/lib/team-membership"
import {
  assertTriggerGrowth,
  assertTriggersRunnable,
  resolveTriggersWrite,
  storedTriggers,
} from "@/lib/action-trigger-rules"
import { syncAutomationMirror } from "@/lib/action-triggers-mirror"
import { BUILTIN_TIDY_UP_ID, BUILTIN_TIDY_UP_NAME } from "@/lib/builtin-actions"
import type { db as Database } from "@/db/connection"

// SLOP-2 compat shim: the `automations.*` router as clients from before the
// merge still call it (iOS <= 0.14.49, Android <= 0.14.50, desktop/CLI <=
// 0.14.58 write every automation through here; without it an owner on an old
// build can neither pause nor delete one while the device keeps firing it).
// A thin ADAPTER, never a second write path: an automation IS one trigger of
// its action, so every write rewrites that action's `triggers` with the same
// rules `actions.update({triggers})` applies (`resolveTriggersWrite`,
// `assertTriggersRunnable`, owner-only) and re-syncs the legacy mirror in the
// same transaction; the reply is the resulting mirror row in the old wire
// shape. Only the touched trigger goes through the rules — its siblings are
// carried over byte-for-byte. Inputs are zod STRIP mode on purpose.
// Delete this file, its registration in routes/api/trpc/$.ts,
// `legacyAutomationTriggerSchema` and automations.test.ts once
// CLIENT_MIN_VERSION_IOS > 0.14.49 and _ANDROID > 0.14.50 and
// _DESKTOP/_CLI > 0.14.58.

type Tx = Parameters<Parameters<typeof Database.transaction>[0]>[0]

const wireColumns = {
  id: automations.id,
  teamId: automations.teamId,
  actionId: automations.actionId,
  deviceId: automations.deviceId,
  enabled: automations.enabled,
  trigger: automations.trigger,
  agent: automations.agent,
  account: automations.account,
  model: automations.model,
  effort: automations.effort,
  sortOrder: automations.sortOrder,
  createdAt: automations.createdAt,
  updatedAt: automations.updatedAt,
}

// The old router's launch fields. `agent` was a contract enum there; a plain
// string here, so an unknown one gets the worded "Unknown agent" refusal of
// the trigger rules instead of a raw zod error.
const launchFieldsSchema = z.object({
  agent: z.string().max(16).nullable().optional(),
  account: z.string().max(64).nullable().optional(),
  model: z.string().max(64).nullable().optional(),
  effort: z.string().max(32).nullable().optional(),
})

const automationActionIdSchema = z
  .string()
  .uuid()
  .or(z.literal(BUILTIN_TIDY_UP_ID))

/** Old clients write the ambient login as `system` OR null and both were
 * stored as NULL; the SLOP-2 migration folded NULL into an OMITTED pin
 * (= unpinned). The adapter keeps that mapping, so an old build never plants
 * an explicit `system` pin it cannot have meant (EXP-1158 gave it a meaning
 * only newer clients know). */
const LEGACY_SYSTEM_ACCOUNT = `system`

function legacyAccount(account: string | null | undefined): string | null {
  const trimmed = account?.trim() ?? ``
  return trimmed === `` || trimmed === LEGACY_SYSTEM_ACCOUNT ? null : trimmed
}

const notFound = () =>
  new TRPCError({ code: `NOT_FOUND`, message: `Automation not found` })
const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })

async function loadMirrorRow(id: string) {
  const { db } = await import(`@/db/connection`)
  const [row] = await db
    .select(wireColumns)
    .from(automations)
    .where(eq(automations.id, id))
    .limit(1)
  if (!row) throw notFound()
  return row
}

interface OwningAction {
  id: string
  teamId: string
  inputs: unknown
  triggers: ActionTrigger[]
}

const actionColumns = {
  id: actions.id,
  teamId: actions.teamId,
  inputs: actions.inputs,
  triggers: actions.triggers,
}

/** The action a mirror row belongs to (`action_id` is TEXT with no FK). */
async function loadOwningAction(actionId: string): Promise<OwningAction | null> {
  if (!z.string().uuid().safeParse(actionId).success) return null
  const { db } = await import(`@/db/connection`)
  const [action] = await db
    .select(actionColumns)
    .from(actions)
    .where(eq(actions.id, actionId))
    .limit(1)
  return action ? { ...action, triggers: storedTriggers(action.triggers) } : null
}

// The target an old client names. `builtin:tidy-up` was automatable before
// the merge; a builtin carries no triggers now, so it maps to the team's REAL
// "Tidy up" row (the migration made one for every team that automated it).
// A team without that row cannot schedule Tidy up from an old build.
async function loadTargetAction(
  actionId: string,
  teamId: string
): Promise<OwningAction> {
  const { db } = await import(`@/db/connection`)
  if (actionId === BUILTIN_TIDY_UP_ID) {
    const [own] = await db
      .select(actionColumns)
      .from(actions)
      .where(
        and(eq(actions.teamId, teamId), eq(actions.name, BUILTIN_TIDY_UP_NAME))
      )
      .limit(1)
    if (!own) {
      throw new TRPCError({
        code: `PRECONDITION_FAILED`,
        message: `Update the app to schedule Tidy up`,
      })
    }
    return { ...own, triggers: storedTriggers(own.triggers) }
  }
  const action = await loadOwningAction(actionId)
  if (!action) throw bad(`Action not found`)
  if (action.teamId !== teamId) throw bad(`Action must belong to the team`)
  return action
}

/** One automation in the old wire form → the trigger `actions.update` takes. */
function asTriggerWrite(args: {
  id?: string
  enabled: boolean
  deviceId: string
  agent: string | null
  account: string | null
  model: string | null
  effort: string | null
  when: z.infer<typeof legacyAutomationTriggerSchema>
}): ActionTriggerInput {
  const runner = {
    ...(args.id ? { id: args.id } : {}),
    enabled: args.enabled,
    deviceId: args.deviceId,
    agent: args.agent,
    account: args.account,
    model: args.model,
    effort: args.effort,
  }
  return args.when.kind === `schedule`
    ? { ...runner, ...args.when }
    : { ...runner, ...args.when }
}

/** The stored when-part in the shape the write schema yields (an event names
 * its source), for an update that leaves `trigger` alone. */
function storedWhen(
  trigger: ActionTrigger
): z.infer<typeof legacyAutomationTriggerSchema> {
  const when = triggerWhenPart(trigger)
  return when.kind === `event`
    ? { ...when, source: (trigger as { source?: `exponential` }).source ?? `exponential` }
    : when
}

async function writeTriggers(
  tx: Tx,
  action: { id: string; teamId: string },
  triggers: ActionTrigger[]
): Promise<void> {
  await tx
    .update(actions)
    .set({ triggers, updatedAt: new Date() })
    .where(eq(actions.id, action.id))
  await syncAutomationMirror(tx, { id: action.id, teamId: action.teamId, triggers })
}

async function mirrorRow(tx: Tx, id: string) {
  const [row] = await tx
    .select(wireColumns)
    .from(automations)
    .where(eq(automations.id, id))
    .limit(1)
  if (!row) throw notFound()
  return row
}

export const automationsRouter = router({
  // Member-gated, straight off the server-written mirror.
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

  // = append ONE trigger to the target action.
  create: authedProcedure
    .input(
      z
        .object({
          teamId: z.string().uuid(),
          actionId: automationActionIdSchema,
          deviceId: automationDeviceIdSchema,
          trigger: legacyAutomationTriggerSchema,
          enabled: z.boolean().optional(),
        })
        .merge(launchFieldsSchema)
    )
    .mutation(async ({ ctx, input }) => {
      await assertTeamOwner(ctx.session.user.id, input.teamId)
      const action = await loadTargetAction(input.actionId, input.teamId)
      // The count is checked on the WHOLE array: only one trigger goes
      // through `resolveTriggersWrite` below.
      assertTriggerGrowth(action.triggers.length + 1, action.triggers.length)
      const [created] = await resolveTriggersWrite({
        written: [
          asTriggerWrite({
            enabled: input.enabled ?? true,
            deviceId: input.deviceId,
            agent: input.agent || null,
            account: legacyAccount(input.account),
            model: input.model || null,
            effort: input.effort || null,
            when: input.trigger,
          }),
        ],
        existing: action.triggers,
        teamId: action.teamId,
        callerUserId: ctx.session.user.id,
      })
      const next = [...action.triggers, created!]
      assertTriggersRunnable(action.inputs, next)

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await writeTriggers(tx, action, next)
        return { automation: await mirrorRow(tx, created!.id), txId }
      })
    }),

  // = rewrite ONE trigger in place (the enable toggle is `{id, enabled}`), or
  // move it to another action when `actionId` names one.
  update: authedProcedure
    .input(
      z
        .object({
          id: z.string().uuid(),
          actionId: automationActionIdSchema.optional(),
          deviceId: automationDeviceIdSchema.optional(),
          trigger: legacyAutomationTriggerSchema.optional(),
          enabled: z.boolean().optional(),
          // Accepted and IGNORED: a mirror row's order is its trigger's
          // position in the action's array.
          sortOrder: z.number().finite().optional(),
        })
        .merge(launchFieldsSchema)
    )
    .mutation(async ({ ctx, input }) => {
      const row = await loadMirrorRow(input.id)
      await assertTeamOwner(ctx.session.user.id, row.teamId)
      const source = await loadOwningAction(row.actionId)
      const current = source?.triggers.find((t) => t.id === input.id)
      if (!source || !current) throw notFound()
      const target =
        input.actionId === undefined || input.actionId === source.id
          ? source
          : await loadTargetAction(input.actionId, row.teamId)
      const moving = target.id !== source.id

      const currentAgent = current.agent ?? null
      const agent = input.agent === undefined ? currentAgent : input.agent || null
      const deviceId = input.deviceId ?? current.deviceId
      const written = asTriggerWrite({
        id: current.id,
        enabled: input.enabled ?? current.enabled,
        deviceId,
        agent,
        // EXP-995 (the old router's rule): a profile belongs to ONE agent on
        // ONE machine — an agent or device switch that names no account
        // drops the old pin, never carries it across.
        account:
          input.account === undefined
            ? agent === currentAgent && deviceId === current.deviceId
              ? (current.account ?? null)
              : null
            : legacyAccount(input.account),
        model:
          input.model === undefined ? (current.model ?? null) : input.model || null,
        effort:
          input.effort === undefined
            ? (current.effort ?? null)
            : input.effort || null,
        when: input.trigger ?? storedWhen(current),
      })
      if (moving) {
        assertTriggerGrowth(target.triggers.length + 1, target.triggers.length)
      }
      // `existing` holds the trigger itself even on a move, so it keeps its
      // id (run history, the device's firing state) and an unchanged binding,
      // pin or filter is accepted as it stands.
      const [updated] = await resolveTriggersWrite({
        written: [written],
        existing: [current],
        teamId: row.teamId,
        callerUserId: ctx.session.user.id,
      })
      const nextTarget = moving
        ? [...target.triggers, updated!]
        : source.triggers.map((t) => (t.id === current.id ? updated! : t))
      assertTriggersRunnable(target.inputs, nextTarget)

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        // Target first: its upsert re-homes the mirror row, so the source's
        // sync finds nothing of its own to delete and the row (with the run
        // history hanging off it) survives the move.
        await writeTriggers(tx, target, nextTarget)
        if (moving) {
          await writeTriggers(
            tx,
            source,
            source.triggers.filter((t) => t.id !== current.id)
          )
        }
        return { automation: await mirrorRow(tx, current.id), txId }
      })
    }),

  // = remove ONE trigger from its action.
  delete: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const row = await loadMirrorRow(input.id)
      await assertTeamOwner(ctx.session.user.id, row.teamId)
      const action = await loadOwningAction(row.actionId)
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        if (action?.triggers.some((t) => t.id === input.id)) {
          await writeTriggers(
            tx,
            action,
            action.triggers.filter((t) => t.id !== input.id)
          )
        }
        // A mirror row whose trigger is already gone (or whose action is):
        // nothing fires from the action side, the row just goes.
        await tx.delete(automations).where(eq(automations.id, input.id))
        return { ok: true as const, txId }
      })
    }),
})
