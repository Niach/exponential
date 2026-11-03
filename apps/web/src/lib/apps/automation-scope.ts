// #VAPP-220 (eApps A1): the LOOP GUARD's scope and the cheap hooks the
// records service and the event stream call. An automation run executes
// inside `runInAutomation`; every record write and event it causes (its
// records write, the events its action emits) sees `inAutomation()` and
// fires nothing (@exp/emcp automations.json `matchCases`). Kept free of the
// broker's imports: the executor (`automations.ts`) loads lazily, only when
// a published package has a matching automation.
import { AsyncLocalStorage } from "node:async_hooks"
import { matchAutomations } from "@exp/emcp/automations"

const scope = new AsyncLocalStorage<{ appId: string; name: string }>()

/** Whether the current call stack runs inside an automation. */
export function inAutomation(): boolean {
  return scope.getStore() !== undefined
}

export function runInAutomation<T>(ctx: { appId: string; name: string }, fn: () => Promise<T>): Promise<T> {
  return scope.run(ctx, fn)
}

export function automationsOf(pkg: unknown): unknown[] {
  const list = pkg && typeof pkg === `object` ? (pkg as { automations?: unknown }).automations : undefined
  return Array.isArray(list) ? list : []
}

export type RecordTriggerOp = `create` | `update` | `delete`

/** A committed record write: fires the PUBLISHED package's matching record
 *  automations (detached, after the caller's transaction). */
export function afterRecordWrite(
  app: { id: string; package: unknown },
  kind: string,
  op: RecordTriggerOp,
  record: Record<string, unknown>
): void {
  if (inAutomation()) return
  if (matchAutomations(automationsOf(app.package), { record: { kind, op } }, false).length === 0) return
  void import(`@/lib/apps/automations`)
    .then((m) => m.fireRecordAutomations(app.id, kind, op, record))
    .catch((err) => console.error(`[automations] record trigger failed:`, err))
}
