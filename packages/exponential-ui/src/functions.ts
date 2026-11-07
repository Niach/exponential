// VAPP-289 (round 9 §6.1, §6.9): the catalog function map as the router and
// the evaluator read it: which functions a catalog declares, their
// `allowedCallers`, `requiresUserActivation` and arg schemas. The Rust core's
// `functions.rs` mirrors it.

import { A2UI_BASIC_CATALOG_ID, coreCatalog } from "./catalog"

export type AllowedCallers = `rendererOrAgent` | `rendererOnly` | `agentOnly`

/** The local `@index` function (§6.2): never routed. */
export const INDEX_FUNCTION = `@index`

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

/** The first issue of `args` against the declared signature (an unknown
 *  arg, a type mismatch); undefined = valid. Null passes (absent). */
export function checkArgs(signature: Signature, args: Record<string, unknown>): string | undefined {
  const declared = signature.args ?? {}
  for (const [k, v] of Object.entries(args)) {
    const raw = own(declared, k)
    if (raw === undefined) return `${k} is not an argument`
    const ty = typeof raw === `string` ? raw : typeof raw === `object` && raw !== null && typeof (raw as { type?: unknown }).type === `string` ? (raw as { type: string }).type : `any`
    const ok = ty === `string` ? typeof v === `string` : ty === `number` || ty === `integer` ? typeof v === `number` : ty === `boolean` ? typeof v === `boolean` : ty === `array` ? Array.isArray(v) : ty === `object` ? typeof v === `object` && v !== null && !Array.isArray(v) : true
    if (!ok && v !== null) return `${k} is not a ${ty}`
  }
  return undefined
}
