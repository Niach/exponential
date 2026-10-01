import { and, eq, inArray, notInArray } from "drizzle-orm"
import { triggerWhenPart, type ActionTrigger } from "@exp/db-schema/domain"
import { actions, automations } from "@/db/schema"
import { storedTriggers } from "@/lib/action-trigger-rules"
import type { db as Database } from "@/db/connection"

type Tx = Parameters<Parameters<typeof Database.transaction>[0]>[0]

/**
 * SLOP-2: keeps the legacy `automations` table equal to one action's
 * `triggers`, inside the transaction that wrote them. Clients and daemons
 * from before the merge still sync that shape and fire from it, so a stale
 * row would keep firing a paused or removed trigger. Rows are UPSERTED by the
 * trigger id (never delete-and-insert): `coding_sessions.automation_id`
 * references them ON DELETE SET NULL, and a rewrite must not unlink the run
 * history of a trigger that merely changed. Goes away with the table.
 */
export async function syncAutomationMirror(
  tx: Tx,
  action: { id: string; teamId: string; triggers: ActionTrigger[] }
): Promise<void> {
  const ids = action.triggers.map((trigger) => trigger.id)
  await tx
    .delete(automations)
    .where(
      ids.length > 0
        ? and(eq(automations.actionId, action.id), notInArray(automations.id, ids))
        : eq(automations.actionId, action.id)
    )
  for (const [index, trigger] of action.triggers.entries()) {
    const row = {
      teamId: action.teamId,
      actionId: action.id,
      deviceId: trigger.deviceId,
      enabled: trigger.enabled,
      trigger: triggerWhenPart(trigger),
      agent: trigger.agent ?? null,
      account: trigger.account ?? null,
      model: trigger.model ?? null,
      effort: trigger.effort ?? null,
      sortOrder: index + 1,
    }
    await tx
      .insert(automations)
      .values({ id: trigger.id, ...row })
      .onConflictDoUpdate({
        target: automations.id,
        set: { ...row, updatedAt: new Date() },
      })
  }
}

/**
 * Pauses every enabled trigger bound to `deviceId` on the given teams'
 * actions (a withdrawn device share), mirror included. Actions with nothing
 * to pause stay untouched (no needless Electric op).
 */
export async function pauseDeviceTriggers(
  tx: Tx,
  teamIds: string[],
  deviceId: string
): Promise<void> {
  const rows = await tx
    .select({ id: actions.id, teamId: actions.teamId, triggers: actions.triggers })
    .from(actions)
    .where(inArray(actions.teamId, teamIds))
  for (const row of rows) {
    const triggers = storedTriggers(row.triggers)
    const bound = (trigger: ActionTrigger) =>
      trigger.enabled && trigger.deviceId === deviceId
    if (!triggers.some(bound)) continue
    const next = triggers.map((trigger) =>
      bound(trigger) ? { ...trigger, enabled: false } : trigger
    )
    await tx
      .update(actions)
      .set({ triggers: next, updatedAt: new Date() })
      .where(eq(actions.id, row.id))
    await syncAutomationMirror(tx, { ...row, triggers: next })
  }
}
