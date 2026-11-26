// VAPP-289 (round 9 §6.1, §6.9): the catalog function map as the router and
// the evaluator read it: which functions a catalog declares, their
// `allowedCallers`, `requiresUserActivation` and arg schemas. The Rust core's
// `functions.rs` mirrors it.

import { A2UI_BASIC_CATALOG_ID, coreCatalog } from "./catalog"
import { MAX_DEPTH } from "./limits"

export type AllowedCallers = `rendererOrAgent` | `rendererOnly` | `agentOnly`

/** The local `@index` function (§6.2): never routed. */
export const INDEX_FUNCTION = `@index`

const isDynamic = (v: unknown): boolean => typeof v === `object` && v !== null && !Array.isArray(v) && (typeof (v as Record<string, unknown>)[`@path`] === `string` || typeof (v as Record<string, unknown>)[`@call`] === `string`)

interface Signature {
  args?: Record<string, unknown>
  returns?: unknown
}

const own = <T>(table: Record<string, T>, key: string): T | undefined => (Object.prototype.hasOwnProperty.call(table, key) ? table[key] : undefined)

/** The signature a catalog declares for `name`; undefined = not declared
 *  there. Core, core-lite and extensions: the core map (+ the basic
 *  validators, which the core declares too); basic: the 14. */
export function declaredFunction(catalogId: string, name: string): Signature | undefined {
  const { core, basic } = coreCatalog.functions
  if (catalogId === A2UI_BASIC_CATALOG_ID) return own(basic, name)
  return own(core, name) ?? own(basic, name)
}

/** §6.1: `set`, `bind`, `openUrl` and every host function are
 *  `rendererOnly`; every other (pure) function `rendererOrAgent`. */
export function allowedCallers(name: string): AllowedCallers {
  if (name === `set` || name === `bind` || name === `openUrl`) return `rendererOnly`
  const { core, basic } = coreCatalog.functions
  return own(core, name) || own(basic, name) ? `rendererOrAgent` : `rendererOnly`
}

/** §6.1: only `openUrl` needs a user gesture. */
export const requiresUserActivation = (name: string): boolean => name === `openUrl`

/** §6.9: why the AGENT may not call `name` (undefined = it may). */
export function agentCallRefusal(name: string): string | undefined {
  if (allowedCallers(name) === `rendererOnly`) return `${name} cannot be called by the agent`
  return requiresUserActivation(name) ? `${name} needs a user gesture` : undefined
}

/** §6.9: the calls NESTED in a callRendererFunction's args, checked before any
 *  of them runs: the first one the agent may not call, or args nested past
 *  `maxDepth` levels. (`${…}` expressions are checked when they evaluate.) */
export function nestedCallRefusal(value: unknown, depth = 1): { code: string; message: string } | undefined {
  if (typeof value !== `object` || value === null) return undefined
  if (depth > MAX_DEPTH) return { code: `EXECUTION_ERROR`, message: `args nested deeper than ${MAX_DEPTH} levels` }
  const name = Array.isArray(value) ? undefined : (value as Record<string, unknown>)[`@call`]
  if (typeof name === `string` && name !== INDEX_FUNCTION) {
    const refused = agentCallRefusal(name)
    if (refused !== undefined) return { code: `INVALID_FUNCTION_CALL`, message: refused }
  }
  for (const v of Object.values(value)) {
    const hit = nestedCallRefusal(v, depth + 1)
    if (hit) return hit
  }
  return undefined
}

/** The first issue of `args` against the declared signature (an unknown
 *  arg, a type mismatch); undefined = valid. Null passes (absent), so does a
 *  binding or a call (its type shows when it evaluates). */
export function checkArgs(signature: Signature, args: Record<string, unknown>): string | undefined {
  const declared = signature.args ?? {}
  for (const [k, v] of Object.entries(args)) {
    const raw = own(declared, k)
    if (raw === undefined) return `${k} is not an argument`
    const ty = typeof raw === `string` ? raw : typeof raw === `object` && raw !== null && typeof (raw as { type?: unknown }).type === `string` ? (raw as { type: string }).type : `any`
    const ok = ty === `string` ? typeof v === `string` : ty === `number` || ty === `integer` ? typeof v === `number` : ty === `boolean` ? typeof v === `boolean` : ty === `array` ? Array.isArray(v) : ty === `object` ? typeof v === `object` && v !== null && !Array.isArray(v) : true
    if (!ok && v !== null && !isDynamic(v)) return `${k} is not a ${ty}`
  }
  return undefined
}
