// #EXP-1510: the orchestration core's SHADOW MODE. Every event the core
// models (packages/eagent: send.start, send.message, session.end,
// device.claim/report/reply/consume/heartbeat, watchdog.tick) runs the
// server's own logic exactly as before; beside it, the state the event saw
// is loaded, eagent-hub-core (WASM) reduces it, and its decision is diffed
// against what the server left (diff.ts). Mismatches are COUNTED per rule
// and UTC day (eagent_shadow_counts) with the day's first samples
// (eagent_shadow_samples); nothing the core decides is ever applied.
//
// Off with EAGENT_SHADOW=off (and under vitest unless EAGENT_SHADOW=on); EAGENT_SHADOW_SAMPLE (0..1, default 1) thins
// it; a missing WASM build turns it off. The state the server left is read
// right after it answered (outside its transaction), the core runs after
// that; a shadow failure is counted, logged and never reaches the caller.

import { randomUUID } from "node:crypto"
import { sql } from "drizzle-orm"
import { ENVELOPE_VERSION } from "@exp/ecore"
import type { HubEvent, HubMessage, HubState } from "@exp/eagent/model"
import { db } from "@/db/connection"
import { eagentShadowCounts, eagentShadowSamples } from "@/db/schema"
import { loadCore } from "./core"
import { diffShadow, isMatch, type ShadowDiff } from "./diff"
import { loadHub, postedSince, type HubScope } from "./hub"

export type ShadowRule =
  | `send.start`
  | `send.message`
  | `session.end`
  | `device.claim`
  | `device.report`
  | `device.reply`
  | `device.consume`
  | `device.heartbeat`
  | `watchdog.tick`

/** Samples kept per rule and UTC day (the counts keep going). */
export const SAMPLES_PER_DAY = 25
/** Comparisons running at once per process; past it an event is counted skipped. */
const MAX_IN_FLIGHT = 4
let inFlight = 0

export function shadowEnabled(env: Record<string, string | undefined> = process.env): boolean {
  if (env.EAGENT_SHADOW === `off`) return false
  // Unit tests mock the database: only a test that asks for it runs the shadow.
  if (env.VITEST && env.EAGENT_SHADOW !== `on`) return false
  const sample = env.EAGENT_SHADOW_SAMPLE === undefined ? 1 : Number(env.EAGENT_SHADOW_SAMPLE)
  return Number.isFinite(sample) && sample > 0 && Math.random() < sample
}

export interface ShadowPlan<T> {
  rule: ShadowRule
  scope: HubScope
  /** The core event's data, built once the server answered (it names the ids the server chose; `after` gives
   *  the inputs the server derived, like a new row's visibility); null = outside what the core models (skipped). */
  data: (result: T | undefined, serverError: string | null, after: HubState) => Record<string, unknown> | null
  /** More to load after (a session the event created). */
  after?: (result: T | undefined) => HubScope
  /** The event's clock (the watchdog passes its own `now`). */
  at?: Date
}

type Tally = `compared` | `mismatched` | `skipped` | `failed`

async function count(rule: ShadowRule, tally: Tally[], sample?: ShadowDiff & { event: unknown }) {
  const day = new Date().toISOString().slice(0, 10)
  const add = (name: Tally) => (tally.includes(name) ? 1 : 0)
  const [row] = await db
    .insert(eagentShadowCounts)
    .values({ rule, day, compared: add(`compared`), mismatched: add(`mismatched`), skipped: add(`skipped`), failed: add(`failed`) })
    .onConflictDoUpdate({
      target: [eagentShadowCounts.rule, eagentShadowCounts.day],
      set: {
        compared: sql`${eagentShadowCounts.compared} + excluded.compared`,
        mismatched: sql`${eagentShadowCounts.mismatched} + excluded.mismatched`,
        skipped: sql`${eagentShadowCounts.skipped} + excluded.skipped`,
        failed: sql`${eagentShadowCounts.failed} + excluded.failed`,
        updatedAt: sql`now()`,
      },
    })
    .returning({ mismatched: eagentShadowCounts.mismatched })
  if (sample && row && row.mismatched <= SAMPLES_PER_DAY) {
    await db.insert(eagentShadowSamples).values({ rule, diff: sample as unknown as Record<string, unknown> })
    console.warn(`[eagent-shadow] ${rule} mismatch`, JSON.stringify(sample))
  }
}

/** `CODE: message` for a tRPC error (access refusals are told apart), else the message. */
const errorText = (error: unknown) => {
  const code = error instanceof Error && `code` in error && typeof error.code === `string` ? `${error.code}: ` : ``
  return `${code}${error instanceof Error ? error.message : String(error)}`.slice(0, 300)
}

/** Redacted event data for a sample: ids and flags, never text. */
function redact(data: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(data)) {
    if (key === `text` || key === `body` || key === `summary` || key === `copies`) out[key] = typeof value === `string` ? `<${value.length}>` : `<…>`
    else if (key === `card` && value && typeof value === `object`) out[key] = { kind: (value as { kind?: unknown }).kind }
    else out[key] = value
  }
  return out
}

interface Settled<T> {
  result: T | undefined
  serverError: string | null
  after: HubState
  posted: HubMessage[]
}

/** What the server left, read right after it answered (before the caller's next call can move it). */
async function observe<T>(
  plan: ShadowPlan<T>,
  before: HubState,
  since: Date,
  result: T | undefined,
  serverError: string | null
): Promise<Settled<T>> {
  const extra = plan.after?.(result) ?? {}
  // Every row the core saw, even one the event moved out of scope (a watcher that forgot an ended session).
  const after = await loadHub({
    ...plan.scope,
    sessionIds: [...(plan.scope.sessionIds ?? []), ...(extra.sessionIds ?? []), ...Object.keys(before.sessions)],
  })
  const posted = await postedSince(Object.keys(after.sessions), since)
  return { result, serverError, after, posted }
}

async function compare<T>(plan: ShadowPlan<T>, before: HubState, at: Date, settled: Settled<T>) {
  const core = await loadCore()
  if (!core) return
  const { result, serverError, after, posted } = settled
  // Access is the server's alone (packages/eagent README: "access checks beyond visibility" are out of the core).
  if (serverError?.startsWith(`FORBIDDEN:`) || serverError?.startsWith(`UNAUTHORIZED:`)) return count(plan.rule, [`skipped`])
  const data = plan.data(result, serverError, after)
  if (data === null) return count(plan.rule, [`skipped`])
  const event: HubEvent = {
    v: ENVELOPE_VERSION,
    id: randomUUID(),
    kind: plan.rule,
    at: at.getTime(),
    source: `server`,
    sessionId: typeof data.sessionId === `string` ? data.sessionId : null,
    data,
  }
  const outcome = core.reduce(before, event)
  const diff = diffShadow({ before, outcome, after, posted, serverError: serverError?.replace(/^[A-Z_]+: /, ``) ?? null })
  if (isMatch(diff)) return count(plan.rule, [`compared`])
  return count(plan.rule, [`compared`, `mismatched`], { ...diff, event: redact(data) })
}

/** Runs `run` (the server's logic, unchanged) and, beside it, the core on the same event. */
export async function shadow<T>(plan: ShadowPlan<T>, run: () => Promise<T>): Promise<T> {
  if (!shadowEnabled() || !(await loadCore())) return run()
  if (inFlight >= MAX_IN_FLIGHT) {
    void count(plan.rule, [`skipped`]).catch(() => {})
    return run()
  }
  inFlight += 1
  let released = false
  const release = () => {
    if (!released) inFlight -= 1
    released = true
  }
  // `since` = the wall clock the server's rows are stamped with; `at` = the event's clock (a watchdog's `now`).
  const since = new Date()
  const at = plan.at ?? since
  let before: HubState
  try {
    before = await loadHub(plan.scope)
  } catch (error) {
    release()
    console.error(`[eagent-shadow] ${plan.rule} load failed`, error)
    void count(plan.rule, [`failed`]).catch(() => {})
    return run()
  }
  const settle = async (result: T | undefined, serverError: string | null) => {
    try {
      const settled = await observe(plan, before, since, result, serverError)
      void compare(plan, before, at, settled)
        .catch((error) => {
          console.error(`[eagent-shadow] ${plan.rule} compare failed`, error)
          return count(plan.rule, [`failed`]).catch(() => {})
        })
        .finally(release)
    } catch (error) {
      release()
      console.error(`[eagent-shadow] ${plan.rule} observe failed`, error)
      void count(plan.rule, [`failed`]).catch(() => {})
    }
  }
  let result: T
  try {
    result = await run()
  } catch (error) {
    await settle(undefined, errorText(error))
    throw error
  }
  await settle(result, null)
  return result
}
