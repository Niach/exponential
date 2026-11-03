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

// #VAPP-236 (A2): the AGENT LOOP GUARD's input. The MCP gate runs every
// call inside the caller's own session (X-Exp-Session-Id, verified); a
// record write or an event it causes names that session, and the executor
// skips the hand-offs to that tree's acting agent (automations.json
// `byAgent`).
const causing = new AsyncLocalStorage<string>()

/** Runs `fn` as caused by `sessionId` (null = by no session). */
export function runAsSession<T>(sessionId: string | null, fn: () => Promise<T>): Promise<T> {
  return sessionId ? causing.run(sessionId, fn) : fn()
}

/** The session the current call stack runs for, if any. */
export function causingSessionId(): string | null {
  return causing.getStore() ?? null
}

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
  const sessionId = causingSessionId()
  void import(`@/lib/apps/automations`)
    .then((m) => m.fireRecordAutomations(app.id, kind, op, record, sessionId))
    .catch((err) => console.error(`[automations] record trigger failed:`, err))
}
