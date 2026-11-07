// VAPP-289 (round 9 contract §3.3, §4.2, §5.2): the ENTRY CHECK every
// component entry passes before any store sees it (the router for
// createSurface/updateComponents, a package install for its templates, a
// card for its nodes). A failing entry is dropped with ONE INVALID_MESSAGE at
// the first failing key; everything the catalog decides stays a reducer
// issue. The Rust core's `check_entry` mirrors this function message for
// message.

import { A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID, CORE_LITE_CATALOG_ID, coreCatalog } from "./catalog"
import { isDataSchema } from "./dynamic"
import { isIdentifier } from "./ids"
import type { ComponentDef, ExtensionDef } from "./types"

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)

/** The envelope keys of a component entry (`ComponentCommon`, §5.1). */
export const ENVELOPE_KEYS: readonly string[] = [`id`, `component`, `catalogId`, `accessibility`, `metadata`]
/** The reserved container `createSurface` implies (§5.4). */
export const SURFACE_COMPONENT = `Surface`
/** The only `@` function name (§6.2). */
export const INDEX_FUNCTION = `@index`

/** RFC 6901 escaping of one reference token. */
export const escapeToken = (token: string): string => token.replace(/~/g, `~0`).replace(/\//g, `~1`)

export interface EntryFailure {
  /** The pointer INSIDE the entry (`/id`, `/text/args/x`); `` = the entry itself. */
  pointer: string
  message: string
}

const quote = (v: string) => JSON.stringify(v)
const notId = (what: string, v: string) => `${what}${quote(v)} is not an identifier (UAX #31)`

/** §3.3: the first malformed directive (or non-identifier function / arg
 *  name) in a value at a DYNAMIC position, depth-first in key order. */
export function checkDynamicValue(value: unknown, pointer = ``): EntryFailure | null {
  if (Array.isArray(value)) {
    for (let i = 0; i < value.length; i++) {
      const hit = checkDynamicValue(value[i], `${pointer}/${i}`)
      if (hit) return hit
    }
    return null
  }
  if (!isObj(value)) return null
  if (`@path` in value) {
    if (typeof value[`@path`] !== `string`) return { pointer: `${pointer}/@path`, message: `@path is a string` }
    for (const k of Object.keys(value)) if (k !== `@path`) return { pointer: `${pointer}/${escapeToken(k)}`, message: `a binding has only @path` }
    return null
  }
  if (`@call` in value) return checkCall(value, pointer)
  for (const [k, v] of Object.entries(value)) {
    if (/^@([^@]|$)/.test(k)) return { pointer: `${pointer}/${escapeToken(k)}`, message: `${quote(k)} is not a directive (@path, @call; @@ escapes a literal @)` }
    const hit = checkDynamicValue(v, `${pointer}/${escapeToken(k)}`)
    if (hit) return hit
  }
  return null
}

/** §3.3: a call `{"@call", args?, catalogId?}` and everything under its args. */
export function checkCall(value: Record<string, unknown>, pointer = ``): EntryFailure | null {
  const name = value[`@call`]
  if (typeof name !== `string`) return { pointer: `${pointer}/@call`, message: `@call is a string` }
  for (const k of Object.keys(value))
    if (k !== `@call` && k !== `args` && k !== `catalogId`) return { pointer: `${pointer}/${escapeToken(k)}`, message: `a call has only @call, args and catalogId` }
  if (name !== INDEX_FUNCTION && !isIdentifier(name)) return { pointer: `${pointer}/@call`, message: notId(`function `, name) }
  if (`catalogId` in value && typeof value.catalogId !== `string`) return { pointer: `${pointer}/catalogId`, message: `catalogId is a string` }
  if (`args` in value) {
    if (!isObj(value.args)) return { pointer: `${pointer}/args`, message: `args is an object` }
    for (const [k, v] of Object.entries(value.args)) {
      if (!isIdentifier(k)) return { pointer: `${pointer}/args/${escapeToken(k)}`, message: notId(`arg `, k) }
      const hit = checkDynamicValue(v, `${pointer}/args/${escapeToken(k)}`)
      if (hit) return hit
    }
  }
  return null
}

/** §5.1: `metadata` = `{extensions?: {<identifier>: any}}`. */
export function checkMetadata(value: unknown, pointer: string): EntryFailure | null {
  if (!isObj(value)) return { pointer, message: `metadata is an object` }
  for (const k of Object.keys(value)) if (k !== `extensions`) return { pointer: `${pointer}/${escapeToken(k)}`, message: `metadata has only extensions` }
  if (`extensions` in value) {
    if (!isObj(value.extensions)) return { pointer: `${pointer}/extensions`, message: `metadata.extensions is an object` }
    for (const k of Object.keys(value.extensions))
      if (!isIdentifier(k)) return { pointer: `${pointer}/extensions/${escapeToken(k)}`, message: notId(`extension key `, k) }
  }
  return null
}

/** An Action (`on.<event>`, basic `action`): its event context and
 *  userMessage are dynamic, its functionCall is a call. */
function checkAction(value: unknown, pointer: string): EntryFailure | null {
  if (!isObj(value)) return checkDynamicValue(value, pointer)
  for (const [k, v] of Object.entries(value)) {
    const at = `${pointer}/${escapeToken(k)}`
    if (k === `functionCall`) {
      if (!isObj(v) || !(`@call` in v)) return { pointer: at, message: `functionCall is a call ({"@call": …})` }
      const hit = checkCall(v, at)
      if (hit) return hit
    } else if (k === `event` && isObj(v)) {
      for (const [ek, ev] of Object.entries(v)) {
        if (ek !== `context` && ek !== `userMessage`) continue
        const hit = checkDynamicValue(ev, `${at}/${ek}`)
        if (hit) return hit
      }
    } else {
      const hit = checkDynamicValue(v, at)
      if (hit) return hit
    }
  }
  return null
}

/** The component definition an entry resolves to, for its DATA positions and
 *  componentId props (core, core-lite and extensions; basic = none). */
export function entryDef(component: string, catalogId: string | undefined, extensions: readonly ExtensionDef[] = []): ComponentDef | undefined {
  const own = (t: Record<string, ComponentDef> | undefined) => (t && Object.prototype.hasOwnProperty.call(t, component) ? t[component] : undefined)
  if (catalogId === CORE_CATALOG_ID || catalogId === CORE_LITE_CATALOG_ID) return own(coreCatalog.components)
  const ext = extensions.find((e) => e.id === catalogId)
  if (ext) return own(ext.components) ?? own(coreCatalog.components)
  return undefined
}

/** The basic catalog's component-id fields (§14.3): `child`, `Tabs.tabs[].child`, `Modal.trigger`/`content`. */
function basicIdRefs(component: string, entry: Record<string, unknown>): [string, unknown][] {
  const out: [string, unknown][] = []
  if (`child` in entry) out.push([`/child`, entry.child])
  if (component === `Modal`) {
    if (`trigger` in entry) out.push([`/trigger`, entry.trigger])
    if (`content` in entry) out.push([`/content`, entry.content])
  }
  if (component === `Tabs` && Array.isArray(entry.tabs))
    entry.tabs.forEach((t, i) => {
      if (isObj(t) && `child` in t) out.push([`/tabs/${i}/child`, t.child])
    })
  return out
}

export interface EntryContext {
  /** The surface's default catalog (createSurface.catalogId). */
  catalogId?: string
  extensions?: readonly ExtensionDef[]
}

/** §5.2: the first reason an entry is refused, or null. Check order: the
 *  entry is an object, `/id`, `/component`, `/catalogId`, `/metadata…`,
 *  then the other keys in key order (prop names, id references, directives,
 *  depth-first). */
export function checkEntry(entry: unknown, ctx: EntryContext = {}): EntryFailure | null {
  if (!isObj(entry)) return { pointer: ``, message: `a component is an object` }
  const id = entry.id
  if (typeof id !== `string`) return { pointer: `/id`, message: `id is a string` }
  if (!isIdentifier(id)) return { pointer: `/id`, message: notId(`id `, id) }
  const component = entry.component
  if (typeof component !== `string`) return { pointer: `/component`, message: `component is a string` }
  if (!isIdentifier(component)) return { pointer: `/component`, message: notId(`component `, component) }
  if (component === SURFACE_COMPONENT) return { pointer: `/component`, message: `Surface is reserved (createSurface implies it)` }
  if (`catalogId` in entry && typeof entry.catalogId !== `string`) return { pointer: `/catalogId`, message: `catalogId is a string` }
  if (`metadata` in entry) {
    const hit = checkMetadata(entry.metadata, `/metadata`)
    if (hit) return hit
  }
  const catalogId = typeof entry.catalogId === `string` ? entry.catalogId : ctx.catalogId
  const basic = catalogId === A2UI_BASIC_CATALOG_ID
  const def = basic ? undefined : entryDef(component, catalogId, ctx.extensions)
  const basicRefs = basic ? new Map(basicIdRefs(component, entry)) : undefined
  for (const [k, v] of Object.entries(entry)) {
    if (k === `id` || k === `component` || k === `catalogId` || k === `metadata`) continue
    const at = `/${escapeToken(k)}`
    if (!isIdentifier(k)) return { pointer: at, message: notId(`prop `, k) }
    if (k === `accessibility`) {
      if (!isObj(v)) continue
      for (const key of [`label`, `description`, `hidden`]) {
        const hit = key in v ? checkDynamicValue(v[key], `${at}/${key}`) : null
        if (hit) return hit
      }
      continue
    }
    if (k === `children`) {
      if (Array.isArray(v)) {
        for (let i = 0; i < v.length; i++) if (typeof v[i] !== `string` || !isIdentifier(v[i])) return { pointer: `${at}/${i}`, message: notId(`child id `, String(v[i])) }
      } else if (isObj(v) && `componentId` in v) {
        if (typeof v.componentId !== `string` || !isIdentifier(v.componentId)) return { pointer: `${at}/componentId`, message: notId(`child id `, String(v.componentId)) }
      }
      continue
    }
    if (k === `slots`) {
      if (isObj(v))
        for (const [slot, ref] of Object.entries(v))
          if (typeof ref !== `string` || !isIdentifier(ref)) return { pointer: `${at}/${escapeToken(slot)}`, message: notId(`slot id `, String(ref)) }
      continue
    }
    if (k === `style`) continue
    if (k === `on`) {
      if (isObj(v))
        for (const [event, action] of Object.entries(v)) {
          const hit = checkAction(action, `${at}/${escapeToken(event)}`)
          if (hit) return hit
        }
      continue
    }
    if (basic && k === `action`) {
      const hit = checkAction(v, at)
      if (hit) return hit
      continue
    }
    if (basicRefs) {
      const refs = [...basicRefs.entries()].filter(([p]) => p === at || p.startsWith(`${at}/`))
      for (const [p, ref] of refs) if (typeof ref !== `string` || !isIdentifier(ref)) return { pointer: p, message: notId(`component id `, String(ref)) }
      if (refs.some(([p]) => p === at)) continue
    }
    const schema = def?.props[k]
    if ((schema?.type as string | undefined) === `componentId`) {
      if (typeof v === `string` && !isIdentifier(v)) return { pointer: at, message: notId(`component id `, v) }
      continue
    }
    if (schema && isDataSchema(schema)) continue
    const hit = checkDynamicValue(v, at)
    if (hit) return hit
  }
  return null
}

/** A component entry's refusal as the router reports it: `INVALID_MESSAGE`
 *  at `/<kind>/components/<index>` + the failing key's pointer. */
export function entryRefusal(kind: string, index: number, failure: EntryFailure): { path: string; message: string } {
  return { path: `/${kind}/components/${index}${failure.pointer}`, message: failure.message }
}
