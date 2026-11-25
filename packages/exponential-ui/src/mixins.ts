// Round 9 §5.1: the envelope mixins. On the wire they ride under `@`
// (`@catalogId`, `@accessibility`, `@metadata`, `@checks`). A leaf module: the
// schema emitter reads it without the entry check's imports.

import type { ComponentDef, PropSchema } from "./types"

/** §5.1: the mixin names. BARE, each is an ordinary prop of a component whose
 *  catalog declares it, and refused everywhere else. */
export const MIXIN_NAMES: readonly string[] = [`catalogId`, `accessibility`, `metadata`, `checks`]

/** §5.1: the source catalogs declare the CHECK MIXIN as an array of the
 *  shared `check` shape (`schema.ts` emits `Checkable` for it); on the wire
 *  it is `@checks`, never a prop named `checks`. */
export function isCheckMixin(schema: PropSchema | undefined): boolean {
  return schema?.type === `array` && schema.items?.shape === `check`
}

/** §5.1: does the component take `@checks`? */
export const isCheckable = (def: ComponentDef | undefined): boolean => isCheckMixin(ownProp(def, `checks`))

/** §5.1: a bare mixin name the component declares as an ordinary prop. */
export function declaresBare(def: ComponentDef | undefined, name: string): boolean {
  const schema = ownProp(def, name)
  return schema !== undefined && !(name === `checks` && isCheckMixin(schema))
}

const ownProp = (def: ComponentDef | undefined, name: string): PropSchema | undefined =>
  def && Object.prototype.hasOwnProperty.call(def.props, name) ? def.props[name] : undefined

/** §5.1: why a bare mixin name is refused. */
export const bareMixin = (name: string): string => `${name} is not a declared prop (write @${name})`
