// EXP-1798: the guard rails on Exponential's destructive MCP ops: one
// explicit id per call, the burst guard, the audit target and the tool-test
// prod refusal. Pure (no node: modules) and locked by `fixtures/deletes.json`
// (its `$comment` is the spec).

import fixture from "../fixtures/deletes.json" with { type: "json" }

/** noun → op → the one argument naming what goes. */
export const DESTRUCTIVE_OPS = fixture.destructiveOps as Readonly<Record<string, Readonly<Record<string, string>>>>
export const DELETE_DESCRIPTION_LINE: string = fixture.descriptionLine
export const DELETE_INSTRUCTIONS_LINE: string = fixture.instructionsLine
export const DELETE_BURST_EVENT: string = fixture.burst.event
export const DELETE_BURST_AUDIT_ERROR: string = fixture.auditError
export const PROD_FLAG: string = fixture.prodFlag

/** The argument a destructive op is named by, or null for any other op. */
export function destructiveKey(noun: string, op: string): string | null {
  return DESTRUCTIVE_OPS[noun]?.[op] ?? null
}

export type SingleIdError = `missing` | `notOne`

/** Is `value` ONE explicit id? null = yes, else why not. */
export function singleIdError(value: unknown): SingleIdError | null {
  if (value === undefined || value === null) return `missing`
  if (typeof value !== `string`) return `notOne`
  const id = value.trim()
  if (id.length === 0) return `missing`
  if (id.length > fixture.maxIdLength) return `notOne`
  if (fixture.forbiddenChars.some((c) => id.includes(c))) return `notOne`
  return null
}

function fill(template: string, values: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (whole, key: string) => (key in values ? String(values[key]) : whole))
}

/** The refusal of a destructive call whose id is not one explicit id. */
export function singleIdMessage(error: SingleIdError, noun: string, op: string, key: string): string {
  return fill(fixture.errors[error], { noun, op, key })
}

export interface BurstLimits {
  max: number
  windowSeconds: number
}

const positiveInt = (raw: string | undefined): number | null => {
  if (!raw || !/^\d+$/.test(raw.trim())) return null
  const n = Number(raw.trim())
  return n > 0 ? n : null
}

/** The limits of this deployment (`burst.env` overrides the defaults). */
export function burstLimits(env: Record<string, string | undefined>): BurstLimits {
  return {
    max: positiveInt(env[fixture.burst.env.max]) ?? fixture.burst.max,
    windowSeconds: positiveInt(env[fixture.burst.env.windowSeconds]) ?? fixture.burst.windowSeconds,
  }
}

/** The refusal of a call past the burst. */
export function burstMessage(limits: BurstLimits): string {
  return fill(fixture.errors.burst, { max: limits.max, window: limits.windowSeconds })
}

export type BurstVerdict = `ok` | `blocked` | `blockedQuiet`

/** One caller key's window: the admitted calls and the last event posted. */
export interface BurstState {
  admitted: number[]
  notifiedAt: number | null
}

export function newBurstState(): BurstState {
  return { admitted: [], notifiedAt: null }
}

/** Admit or refuse a destructive call at `nowMs` (mutates `state`).
 *  `blocked` = refuse and post the event; `blockedQuiet` = refuse only. */
export function burstAdmit(state: BurstState, nowMs: number, limits: BurstLimits): BurstVerdict {
  const windowMs = limits.windowSeconds * 1000
  const cutoff = nowMs - windowMs
  state.admitted = state.admitted.filter((at) => at > cutoff)
  if (state.admitted.length < limits.max) {
    state.admitted.push(nowMs)
    return `ok`
  }
  if (state.notifiedAt !== null && state.notifiedAt > cutoff) return `blockedQuiet`
  state.notifiedAt = nowMs
  return `blocked`
}

const PREFIX = `exponential_`

/** The noun and op of a call on a wire tool, if the op is destructive. */
export function destructiveCall(
  tool: string,
  args: unknown
): { noun: string; op: string; key: string; value: unknown } | null {
  if (!tool.startsWith(PREFIX) || !args || typeof args !== `object` || Array.isArray(args)) return null
  const name = tool.slice(PREFIX.length)
  const noun = name.endsWith(`_write`) && !(name in DESTRUCTIVE_OPS) ? name.slice(0, -`_write`.length) : name
  const op = (args as { op?: unknown }).op
  if (typeof op !== `string`) return null
  const key = destructiveKey(noun, op)
  if (!key) return null
  return { noun, op, key, value: (args as Record<string, unknown>)[key] }
}

/** The audit row's `target`: `<op>:<id>` of a destructive call, else null. */
export function destructiveTarget(tool: string, args: unknown): string | null {
  const call = destructiveCall(tool, args)
  if (!call) return null
  const id = typeof call.value === `string` ? call.value.slice(0, fixture.maxIdLength) : `?`
  return `${call.op}:${id}`
}

export type ProdBaseVerdict = `ok` | `refused` | `invalid`

/** May a tool test run against `base`? */
export function prodBaseVerdict(base: string, flag: boolean): ProdBaseVerdict {
  let host: string
  try {
    host = new URL(base).hostname.toLowerCase()
  } catch {
    return `invalid`
  }
  if (fixture.prodHosts.includes(host) && !flag) return `refused`
  return `ok`
}
