import { randomUUID } from "node:crypto"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray } from "drizzle-orm"
import {
  MAX_ACTION_TRIGGERS,
  triggerWhenPart,
  type ActionTrigger,
  type ActionTriggerInput,
  type AutomationTrigger,
} from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import {
  boards,
  devices,
  issueStatuses,
  labels,
  teamMembers,
} from "@/db/schema"

// Action triggers (SLOP-2; the automations of EXP-530/EXP-583 folded into
// their action): the write rules `actions.update({triggers})` applies to each
// trigger. There is NO server scheduler — the bound device selects its
// enabled triggers off the `actions` shape and self-starts the run
// (codingSessions.start with startedReason + automationId = the trigger id).

const codingAgentValues: readonly string[] = contract.codingAgent.values
const agentModelValues: Record<string, readonly string[]> = {
  claude: contract.codingModel.values,
  codex: contract.codexModel.values,
}
const agentEffortValues: Record<string, readonly string[]> = {
  claude: contract.codingEffort.values,
  codex: contract.codexEffort.values,
}

const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })

// A triggered run fills in NO input values, so an action declaring a required
// input while an ENABLED trigger sits on it would run a prompt referencing
// values nobody ever provided: refused at write time, whichever side changes.
// Tolerant reader: `inputs` is jsonb from possibly newer clients.
export const TRIGGER_REQUIRED_INPUTS_MESSAGE = `Triggers can't run actions with required inputs. Make the inputs optional first.`

export function hasRequiredInput(inputs: unknown): boolean {
  if (!Array.isArray(inputs)) return false
  return inputs.some(
    (def) =>
      Boolean(def) &&
      typeof def === `object` &&
      (def as { required?: unknown }).required === true
  )
}

export function assertTriggersRunnable(
  inputs: unknown,
  triggers: readonly { enabled: boolean }[]
): void {
  if (triggers.some((trigger) => trigger.enabled) && hasRequiredInput(inputs)) {
    throw bad(TRIGGER_REQUIRED_INPUTS_MESSAGE)
  }
}

/** Blank or the retired `system` (the ambient login, never used any more) =
 *  unpinned (unset); anything else is stored verbatim. */
function normalizeAccount(account: string | null | undefined): string | null {
  const trimmed = account?.trim() ?? ``
  return trimmed === `` || trimmed === `system` ? null : trimmed
}

// Agent/model/effort: unset = the device's launch defaults. A model/effort is
// only meaningful against an agent, so both are validated against the
// (agent ?? claude) contract lists, mirroring steer.startSession.
// EXP-995: `account` = the agent PROFILE id on the bound device
// (`agent_profiles`, what a start passes as `account`). It names a directory
// under ONE agent's config root, so it needs the agent pinned beside it.
// Unset = unpinned = the profile the machine LAST USED for that agent.
// Which profiles a machine holds is device-local (the heartbeat's
// `agent_accounts` may lag), so the id itself is not checked here — the
// runner falls back to its last used profile for one it no longer has.
// A pin is validated only when it CHANGED against the stored trigger with
// the same id (or, for model/effort/account, when the agent they hang off
// changed): a trigger migrated with a model since retired from the contract
// must not make every later triggers write on its action fail — pausing it,
// or editing a sibling, included.
interface LaunchPins {
  agent: string | null
  account: string | null
  model: string | null
  effort: string | null
}

function assertLaunchFields(fields: LaunchPins, stored?: LaunchPins): void {
  const changed = (key: keyof LaunchPins) =>
    !stored || fields[key] !== stored[key]
  const agentChanged = changed(`agent`)
  if (agentChanged && fields.agent && !codingAgentValues.includes(fields.agent)) {
    throw bad(`Unknown agent`)
  }
  const agent = fields.agent ?? `claude`
  if ((agentChanged || changed(`account`)) && fields.account && !fields.agent) {
    throw bad(`An account pin needs its agent pinned`)
  }
  if (
    (agentChanged || changed(`model`)) &&
    fields.model &&
    !(agentModelValues[agent] ?? []).includes(fields.model)
  ) {
    throw bad(`Unknown ${agent} model`)
  }
  if (
    (agentChanged || changed(`effort`)) &&
    fields.effort &&
    !(agentEffortValues[agent] ?? []).includes(fields.effort)
  ) {
    throw bad(`Unknown ${agent} effort`)
  }
}

/** The pins a stored trigger carries, in the written (null = unset) form. */
function storedPins(trigger: ActionTrigger): LaunchPins {
  return {
    agent: trigger.agent ?? null,
    account: trigger.account ?? null,
    model: trigger.model ?? null,
    effort: trigger.effort ?? null,
  }
}

/**
 * `MAX_ACTION_TRIGGERS` bounds GROWTH only: a write is refused when it holds
 * more triggers than the cap AND more than the action already stores. The
 * SLOP-2 migration folded every automation of an action into its array with
 * no bound, and such an action must still accept a pause, an edit or a
 * removal. Shared by `actions.update` and the legacy `automations.*` adapter.
 */
export function assertTriggerGrowth(nextCount: number, storedCount: number): void {
  if (nextCount > MAX_ACTION_TRIGGERS && nextCount > storedCount) {
    throw bad(`An action can have at most ${MAX_ACTION_TRIGGERS} triggers`)
  }
}

// The runner binding: `deviceId` is the steer TEXT id (devices.device_id) —
// unique per (userId, deviceId), so the same id can exist under several
// users: usable when any matching row is the caller's own (any kind), or is a
// SERVER registration shared with THIS team by an owner who is still a member
// — the same shared-row gate `visibleDeviceRows` (EXP-639) and steer's
// `resolveTargetDevice` apply, so a teammate's desktop can never be bound to
// a trigger it would never surface in a picker. The device must advertise
// the `automations` cap — an ACTION cap (EXP-409), so its absence really means
// no agent is signed in on that machine, which is what the refusal says — and,
// when an agent is pinned, advertise that agent.
export async function assertDeviceUsable(
  deviceId: string,
  teamId: string,
  callerUserId: string,
  agent: string | null | undefined
): Promise<void> {
  const { db } = await import(`@/db/connection`)
  const rows = await db
    .select({
      userId: devices.userId,
      sharedTeamIds: devices.sharedTeamIds,
      kind: devices.kind,
      caps: devices.caps,
      agents: devices.agents,
    })
    .from(devices)
    .where(eq(devices.deviceId, deviceId))
  let usableRows = rows.filter((row) => row.userId === callerUserId)
  if (usableRows.length === 0) {
    const shared = rows.filter(
      (row) =>
        (row.sharedTeamIds ?? []).includes(teamId) && row.kind === `server`
    )
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
    throw bad(`Trigger device must be yours or shared with this team`)
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
      throw bad(`Trigger ${what} must belong to the team`)
    }
  }
  await assertIdsInTeam(trigger.filters?.boardIds, boards, `boards`)
  await assertIdsInTeam(trigger.filters?.labelIds, labels, `labels`)
  await assertIdsInTeam(trigger.filters?.toStatusIds, issueStatuses, `statuses`)
}

/** Tolerant: the stored array as a list of objects that carry a string id. */
export function storedTriggers(value: unknown): ActionTrigger[] {
  if (!Array.isArray(value)) return []
  return value.filter(
    (trigger): trigger is ActionTrigger =>
      Boolean(trigger) &&
      typeof trigger === `object` &&
      typeof (trigger as { id?: unknown }).id === `string`
  )
}

/**
 * Turns a written triggers array into the stored one. A trigger keeps its id
 * only when the action already holds it (ids are also the legacy mirror's
 * primary keys, so a foreign or made-up id is re-minted); unset pins are
 * omitted. A binding that did not change is accepted as-is: co-owners can
 * toggle a trigger bound to a teammate's private device they could never
 * re-bind, and a filter whose label was since deleted does not block a pause.
 * The same holds for pins (`assertLaunchFields`) and for the trigger count
 * (`assertTriggerGrowth`).
 */
export async function resolveTriggersWrite(args: {
  written: ActionTriggerInput[]
  existing: ActionTrigger[]
  teamId: string
  callerUserId: string
}): Promise<ActionTrigger[]> {
  assertTriggerGrowth(args.written.length, args.existing.length)
  const previousById = new Map(args.existing.map((t) => [t.id, t]))
  const resolved: ActionTrigger[] = []
  for (const written of args.written) {
    const previous = written.id ? previousById.get(written.id) : undefined
    const pins = {
      agent: written.agent || null,
      account: normalizeAccount(written.account),
      model: written.model || null,
      effort: written.effort || null,
    }
    assertLaunchFields(pins, previous ? storedPins(previous) : undefined)
    const runner = {
      id: previous?.id ?? randomUUID(),
      enabled: written.enabled,
      deviceId: written.deviceId,
      ...(pins.agent ? { agent: pins.agent } : {}),
      ...(pins.account ? { account: pins.account } : {}),
      ...(pins.model ? { model: pins.model } : {}),
      ...(pins.effort ? { effort: pins.effort } : {}),
    }
    const next: ActionTrigger =
      written.kind === `schedule`
        ? {
            ...runner,
            kind: `schedule`,
            interval: written.interval,
            minuteOfDay: written.minuteOfDay,
            ...(written.weekday !== undefined ? { weekday: written.weekday } : {}),
            ...(written.dayOfMonth !== undefined
              ? { dayOfMonth: written.dayOfMonth }
              : {}),
          }
        : {
            ...runner,
            kind: `event`,
            source: written.source,
            event: written.event,
            ...(written.filters ? { filters: written.filters } : {}),
          }
    if (
      !previous ||
      previous.deviceId !== next.deviceId ||
      (next.agent && next.agent !== previous.agent)
    ) {
      await assertDeviceUsable(
        next.deviceId,
        args.teamId,
        args.callerUserId,
        next.agent
      )
    }
    const when = triggerWhenPart(next)
    if (
      !previous ||
      JSON.stringify(triggerWhenPart(previous)) !== JSON.stringify(when)
    ) {
      await assertFiltersInTeam(when, args.teamId)
    }
    resolved.push(next)
  }
  return resolved
}
