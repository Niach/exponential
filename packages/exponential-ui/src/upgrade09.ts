// VAPP-289 (round 9 contract §14): `upgrade09`, the PURE A2UI v0.9 → 1.0
// converter. It bridges 0.9 input on every core entry while `ACCEPT_V09`
// holds and then serves only the stored-data migration
// (`apps/web/scripts/a2ui10-migrate.ts`). The Rust core mirrors it
// (`eui-core/src/upgrade09.rs`); fixtures/upgrade09.json locks both.
//
// Rules (§14.3): the envelope, the dynamic forms (`{path}` → `{"@path"}`,
// `{call, args, returnType}` → `{"@call", args}`, `@` literal keys escaped),
// the presence wraps, the basic component rules, the flat host function
// names and the identifier re-key with its `_2` collision rule. Output key
// order follows the input's: a renamed key keeps its place, an added key
// goes to the end of its object (`version` is the exception: first).

import { coreCatalog } from "./catalog"
import { isIdentifier, mangle } from "./ids"
import { verifyPackageFile } from "./host/signature"
import type { PackageFileVerification } from "./host/signature"

type Obj = Record<string, unknown>

const isObj = (v: unknown): v is Obj => typeof v === `object` && v !== null && !Array.isArray(v)
const own = (o: Obj, k: string) => Object.prototype.hasOwnProperty.call(o, k)

/** The two 0.9 basic catalog ids and the 1.0 one they become. */
export const BASIC_CATALOG_IDS_09: readonly string[] = [
  `https://a2ui.org/specification/v0_9/basic_catalog.json`,
  `https://a2ui.org/specification/v0_9/catalogs/basic/catalog.json`,
]
export const BASIC_CATALOG_ID_10 = `https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json`

const isBasicId = (id: unknown) => id === BASIC_CATALOG_ID_10 || BASIC_CATALOG_IDS_09.includes(id as string)
const upgradeCatalogId = (id: unknown) => (BASIC_CATALOG_IDS_09.includes(id as string) ? BASIC_CATALOG_ID_10 : id)

/** `surfaces`: per surface, every id seen so far → its 1.0 id (kept ids map
 *  to themselves). `catalogs`: the catalog a `createSurface` named, so a
 *  later `updateComponents` of a basic surface gets the basic rules. */
export interface Upgrade09State {
  surfaces: Record<string, Record<string, string>>
  catalogs: Record<string, string>
}

export function emptyUpgrade09State(): Upgrade09State {
  return { surfaces: {}, catalogs: {} }
}

function cloneState(state: Upgrade09State | undefined): Upgrade09State {
  const out = emptyUpgrade09State()
  if (!state) return out
  for (const [sid, map] of Object.entries(state.surfaces ?? {})) out.surfaces[sid] = { ...map }
  Object.assign(out.catalogs, state.catalogs ?? {})
  return out
}

// ---------------------------------------------------------------------------
// Host function names (§6.3, §14.3)

const DOT_PREFIXES = [`app`, `records`, `actions`, `harness`, `exponential`, `camera`, `clipboard`, `files`, `http`, `location`, `notifications`, `photos`, `print`, `share`]

/** Package tool ids re-keyed by `upgradePackageBody09` (old → new). */
type ToolMap = Record<string, string>

function renameWith(name: string, tools: ToolMap): string {
  const dot = name.indexOf(`.`)
  if (dot < 0) return name
  const head = name.slice(0, dot)
  if (head === `tools` || head === `upstream`) {
    const rest = name.slice(dot + 1)
    const second = rest.indexOf(`.`)
    const id = second < 0 ? rest : rest.slice(0, second)
    const flatId = head === `tools` && own(tools, id) ? tools[id]! : mangle(id)
    return second < 0 ? `${head}_${flatId}` : `${head}_${flatId}__${mangle(rest.slice(second + 1))}`
  }
  return DOT_PREFIXES.includes(head) ? name.replace(/\./g, `_`) : name
}

/** §14.3 `rename`: a 0.9 host function name → its flat 1.0 name; any other
 *  name unchanged. */
export function rename09(name: string): string {
  return renameWith(name, {})
}

function renamePatternWith(pattern: string, tools: ToolMap): string {
  if (!pattern.endsWith(`*`)) return renameWith(pattern, tools)
  const base = pattern.slice(0, -1)
  if (!base.endsWith(`.`)) return `${renameWith(base, tools)}*`
  const stem = base.slice(0, -1)
  const segments = stem.split(`.`)
  if (segments.length === 1) return DOT_PREFIXES.includes(stem) || stem === `tools` || stem === `upstream` ? `${stem}_*` : pattern
  const flat = renameWith(stem, tools)
  if (flat === stem) return pattern
  // `tools.<id>.*` / `upstream.<server>.*`: the tool name follows `__`.
  const sep = (segments[0] === `tools` || segments[0] === `upstream`) && segments.length === 2 ? `__` : `_`
  return `${flat}${sep}*`
}

/** §14.3: a policy / `functions` pattern (an exact name or a prefix ending
 *  in `*`) renamed, the `*` kept: `harness.*` → `harness_*`,
 *  `tools.crm.*` → `tools_crm__*`, `*` unchanged. */
export function renamePattern09(pattern: string): string {
  return renamePatternWith(pattern, {})
}

// ---------------------------------------------------------------------------
// Dynamic values (§14.3 "Dynamic conversion", "Presence wrap")

const VALIDATORS = new Set([`required`, `regex`, `length`, `numeric`, `email`])

function returnsOf(name: string): string | undefined {
  if (VALIDATORS.has(name)) return `validationResult`
  const fns = coreCatalog.functions
  if (own(fns.core, name)) return fns.core[name]!.returns
  if (own(fns.basic, name)) return fns.basic[name]!.returns
  return undefined
}

const present = (v: unknown) => v !== undefined && v !== null && v !== false && v !== ``

type PresenceColumn = `visible` | `check` | `bool`

const cond = (x: unknown) => ({ "@call": `cond`, args: { if: x, then: true, else: false } })

/** The presence wrap table: `x` is already 1.0. */
function asPresence(x: unknown, column: PresenceColumn): unknown {
  if (isObj(x) && own(x, `@path`)) return cond(x)
  if (isObj(x) && own(x, `@call`)) {
    const r = typeof x[`@call`] === `string` ? returnsOf(x[`@call`] as string) : undefined
    return r === `boolean` || r === `validationResult` ? x : cond(x)
  }
  if (column === `check`) return cond(x)
  if (typeof x === `boolean`) return x
  return present(x)
}

interface Ctx {
  tools: ToolMap
}

/** A 0.9 binding or call, as `dyn` reads them. */
const isDirective09 = (v: unknown): boolean => isObj(v) && ((Object.keys(v).length === 1 && typeof v.path === `string`) || typeof v.call === `string`)

/** §14.3 `dyn`. */
function dyn(v: unknown, ctx: Ctx): unknown {
  if (Array.isArray(v)) return v.map((item) => dyn(item, ctx))
  if (!isObj(v)) return v
  const keys = Object.keys(v)
  if (keys.length === 1 && keys[0] === `path` && typeof v.path === `string`) return { "@path": v.path }
  if (typeof v.call === `string`) {
    const name = renameWith(v.call, ctx.tools)
    const out: Obj = { "@call": name }
    if (isObj(v.args)) out.args = dynArgs(name, v.args, ctx)
    return out
  }
  const out: Obj = {}
  for (const [k, item] of Object.entries(v)) out[k.startsWith(`@`) ? `@${k}` : k] = dyn(item, ctx)
  return out
}

/** A call's args: every value `dyn`'d (the keys are arg names, kept), with
 *  the presence wrap on `and.values[i]`, `or.values[i]`, `not.value`. */
function dynArgs(name: string, args: Obj, ctx: Ctx): Obj {
  const out: Obj = {}
  for (const [k, item] of Object.entries(args)) {
    let value = dyn(item, ctx)
    if ((name === `and` || name === `or`) && k === `values` && Array.isArray(value)) value = value.map((x) => asPresence(x, `bool`))
    else if (name === `not` && k === `value`) value = asPresence(value, `bool`)
    out[k] = value
  }
  return out
}

/** An Action (`on.<event>` or basic `action`): event context values and
 *  `userMessage` `dyn`'d, `functionCall` → the call object. */
function upgradeAction(action: unknown, ctx: Ctx): unknown {
  if (!isObj(action)) return action
  const out: Obj = {}
  for (const [k, v] of Object.entries(action)) {
    if (k === `event` && isObj(v)) {
      const event: Obj = {}
      for (const [ek, ev] of Object.entries(v)) {
        if (ek === `context` && isObj(ev)) {
          const context: Obj = {}
          for (const [ck, cv] of Object.entries(ev)) context[ck] = dyn(cv, ctx)
          event[ek] = context
        } else event[ek] = ek === `userMessage` ? dyn(ev, ctx) : ev
      }
      out[k] = event
    } else out[k] = k === `functionCall` ? dyn(v, ctx) : v
  }
  return out
}

function upgradeChecks(checks: unknown, ctx: Ctx): unknown {
  if (!Array.isArray(checks)) return dyn(checks, ctx)
  return checks.map((check) => {
    if (!isObj(check)) return dyn(check, ctx)
    const out: Obj = {}
    for (const [k, v] of Object.entries(check)) out[k] = k === `condition` ? asPresence(dyn(v, ctx), `check`) : v
    return out
  })
}

function upgradeAccessibility(a11y: unknown, ctx: Ctx): unknown {
  if (!isObj(a11y)) return a11y
  const out: Obj = {}
  for (const [k, v] of Object.entries(a11y)) out[k] = k === `label` || k === `description` ? dyn(v, ctx) : v
  return out
}

// ---------------------------------------------------------------------------
// Catalog lookups (core component prop schemas)

interface LooseSchema {
  type?: string
  shape?: string
  items?: LooseSchema
}

/** `isDataSchema` (src/dynamic.ts), restated so this module reads the
 *  catalog only: a shape-less object, or an array of data. */
function isDataSchema(schema: LooseSchema | undefined): boolean {
  if (!schema) return false
  if (schema.type === `object`) return schema.shape === undefined
  if (schema.type === `array`) return schema.items !== undefined && isDataSchema(schema.items)
  return false
}

/** The core props that hold a component id (§5.3). */
const CORE_ID_PROPS: Record<string, readonly string[]> = { Label: [`for`], Popover: [`anchor`], Menu: [`anchor`] }

function corePropSchema(component: unknown, prop: string): LooseSchema | undefined {
  if (typeof component !== `string` || !own(coreCatalog.components, component)) return undefined
  const props = coreCatalog.components[component]!.props as Record<string, LooseSchema> | undefined
  return props && own(props, prop) ? props[prop] : undefined
}

function isCoreIdProp(component: unknown, prop: string): boolean {
  if (typeof component !== `string`) return false
  if (CORE_ID_PROPS[component]?.includes(prop)) return true
  return corePropSchema(component, prop)?.type === `componentId`
}

const BASIC_ID_PROPS = new Set([`child`, `trigger`, `content`])

// ---------------------------------------------------------------------------
// Id re-key (§14.3)

class Rekey {
  /** target → its original id (the map is injective). */
  private readonly owners = new Map<string, string>()

  constructor(private readonly map: Record<string, string>) {
    for (const [old, target] of Object.entries(map)) this.owners.set(target, old)
  }

  static forSurface(state: Upgrade09State, surfaceId: string): Rekey {
    return new Rekey((state.surfaces[surfaceId] ??= {}))
  }

  private targetOfOther(id: string): boolean {
    const owner = this.owners.get(id)
    return owner !== undefined && owner !== id
  }

  private assign(old: string, kept: ReadonlySet<string>): string {
    if (own(this.map, old)) return this.map[old]!
    let target: string
    if (isIdentifier(old) && !this.targetOfOther(old)) target = old
    else {
      const base = mangle(old)
      target = base
      for (let n = 2; kept.has(target) || this.owners.has(target); n++) target = `${base}_${n}`
    }
    this.map[old] = target
    this.owners.set(target, old)
    return target
  }

  /** Steps 1–3: the defined ids, in definition order. */
  define(ids: readonly string[]): void {
    const kept = new Set<string>()
    for (const id of ids) if (!own(this.map, id) && isIdentifier(id) && !this.targetOfOther(id)) kept.add(id)
    for (const id of ids) if (kept.has(id)) this.assign(id, kept)
    for (const id of ids) this.assign(id, kept)
  }

  /** Step 4: a reference (assigned on first sight when undefined). */
  ref(id: unknown): unknown {
    return typeof id === `string` ? this.assign(id, new Set()) : id
  }
}

function rekeyChildren(children: unknown, rekey: Rekey): unknown {
  if (Array.isArray(children)) return children.map((c) => rekey.ref(c))
  if (isObj(children)) {
    const out: Obj = {}
    for (const [k, v] of Object.entries(children)) out[k] = k === `componentId` ? rekey.ref(v) : v
    return out
  }
  return children
}

function rekeySlots(slots: unknown, rekey: Rekey): unknown {
  if (!isObj(slots)) return slots
  const out: Obj = {}
  for (const [k, v] of Object.entries(slots)) out[k] = rekey.ref(v)
  return out
}

function rekeyTemplate(template: unknown, rekey: Rekey): unknown {
  if (!isObj(template)) return template
  const out: Obj = {}
  for (const [k, v] of Object.entries(template)) out[k] = k === `component` ? rekey.ref(v) : v
  return out
}

// ---------------------------------------------------------------------------
// Components

const HEADING = /^h([1-5])$/

/** The basic rules that run before `dyn` (Icon, Text, TextField). Returns
 *  the entry with them applied (key order kept), plus the extra check a
 *  `validationRegexp` turns into. */
function basicBefore(entry: Obj, ctx: Ctx): { entry: Obj; extraCheck?: Obj } {
  const component = entry.component
  if (component === `Icon` && isObj(entry.name) && Object.keys(entry.name).length === 1 && own(entry.name, `path`)) {
    return { entry: { ...entry, name: { svgPath: entry.name.path } } }
  }
  if (component === `Text` && typeof entry.variant === `string` && HEADING.test(entry.variant)) {
    const hashes = `${`#`.repeat(Number(HEADING.exec(entry.variant)![1]))} `
    const out: Obj = {}
    for (const [k, v] of Object.entries(entry)) {
      if (k === `variant`) continue
      // The dynamic form is written in 0.9 shape: the prop's own `dyn` converts it.
      if (k === `text`) out.text = typeof v === `string` ? `${hashes}${v}` : { call: `concat`, args: { values: [hashes, v] } }
      else out[k] = v
    }
    return { entry: out }
  }
  if (component === `TextField` && own(entry, `validationRegexp`)) {
    const out: Obj = {}
    for (const [k, v] of Object.entries(entry)) if (k !== `validationRegexp`) out[k] = v
    const value = entry.value
    const bound = isObj(value) && Object.keys(value).length === 1 && typeof value.path === `string`
    const v = bound ? dyn(value, ctx) : { "@path": entry.id }
    return { entry: out, extraCheck: { condition: { "@call": `regex`, args: { value: v, pattern: entry.validationRegexp } }, message: `Invalid value` } }
  }
  return { entry }
}

/** One component's props (flat entry keys, or a card node's `props`). */
function upgradeProp(key: string, value: unknown, component: unknown, basic: boolean, rekey: Rekey, ctx: Ctx): unknown {
  if (key === `checks`) return upgradeChecks(value, ctx)
  if (key === `on`) {
    if (!isObj(value)) return value
    const out: Obj = {}
    for (const [event, action] of Object.entries(value)) out[event] = upgradeAction(action, ctx)
    return out
  }
  if (basic) {
    if (key === `action`) return upgradeAction(value, ctx)
    if (BASIC_ID_PROPS.has(key)) return rekey.ref(value)
    if (key === `tabs` && component === `Tabs` && Array.isArray(value)) {
      return value.map((tab) => {
        if (!isObj(tab)) return dyn(tab, ctx)
        const out: Obj = {}
        for (const [k, v] of Object.entries(tab)) out[k] = k === `child` ? rekey.ref(v) : dyn(v, ctx)
        return out
      })
    }
    return dyn(value, ctx)
  }
  if (isCoreIdProp(component, key)) return rekey.ref(value)
  // A binding or call AT a DATA position still resolves (`resolveProp`): only
  // what is INSIDE a literal there is data.
  if (isDataSchema(corePropSchema(component, key))) return isDirective09(value) ? dyn(value, ctx) : value
  return dyn(value, ctx)
}

function upgradeFlatEntry(raw: unknown, surfaceCatalog: unknown, rekey: Rekey, ctx: Ctx): unknown {
  if (!isObj(raw)) return raw
  const basic = isBasicId(own(raw, `catalogId`) ? raw.catalogId : surfaceCatalog)
  const { entry, extraCheck } = basic ? basicBefore(raw, ctx) : { entry: raw, extraCheck: undefined }
  const out: Obj = {}
  for (const [k, v] of Object.entries(entry)) {
    switch (k) {
      case `id`:
        out.id = rekey.ref(v)
        break
      case `component`:
      case `style`:
      case `metadata`:
        out[k] = v
        break
      case `catalogId`:
        out[k] = upgradeCatalogId(v)
        break
      case `children`:
        out[k] = rekeyChildren(v, rekey)
        break
      case `slots`:
        out[k] = rekeySlots(v, rekey)
        break
      case `template`:
        out[k] = rekeyTemplate(v, rekey)
        break
      case `visible`:
        out[k] = asPresence(dyn(v, ctx), `visible`)
        break
      case `accessibility`:
        out[k] = upgradeAccessibility(v, ctx)
        break
      default:
        out[k] = upgradeProp(k, v, entry.component, basic, rekey, ctx)
    }
  }
  if (extraCheck) out.checks = Array.isArray(out.checks) ? [...out.checks, extraCheck] : [extraCheck]
  if (basic && entry.component === `TextField` && !own(entry, `value`)) out.value = ``
  return out
}

function upgradeComponentsWith(components: unknown, catalogId: unknown, state: Upgrade09State, surfaceId: string, ctx: Ctx): unknown {
  if (!Array.isArray(components)) return components
  const rekey = Rekey.forSurface(state, surfaceId)
  rekey.define(components.flatMap((c) => (isObj(c) && typeof c.id === `string` ? [c.id] : [])))
  return components.map((c) => upgradeFlatEntry(c, catalogId, rekey, ctx))
}

/** A flat 0.9 component list (an `updateComponents` / `createSurface` list
 *  or a package template) upgraded as ONE surface: the state is copied,
 *  never mutated. */
export function upgradeComponents09(components: unknown, catalogId: unknown, state: Upgrade09State = emptyUpgrade09State(), surfaceId = ``): { components: unknown; state: Upgrade09State } {
  const next = cloneState(state)
  return { components: upgradeComponentsWith(components, catalogId, next, surfaceId, { tools: {} }), state: next }
}

// ---------------------------------------------------------------------------
// Messages

function withVersion(rest: Obj): Obj {
  return { version: `1.0`, ...rest }
}

function upgradeCreateSurface(body: Obj, state: Upgrade09State, ctx: Ctx): Obj {
  const surfaceId = typeof body.surfaceId === `string` ? body.surfaceId : ``
  const catalogId = upgradeCatalogId(body.catalogId)
  if (typeof catalogId === `string`) state.catalogs[surfaceId] = catalogId
  const out: Obj = {}
  for (const [k, v] of Object.entries(body)) {
    if (k === `theme`) continue
    if (k === `catalogId`) out[k] = catalogId
    else if (k === `components`) out[k] = upgradeComponentsWith(v, catalogId, state, surfaceId, ctx)
    else out[k] = v
  }
  if (own(body, `theme`)) {
    const metadata: Obj = isObj(out.metadata) ? { ...out.metadata } : {}
    const extensions: Obj = isObj(metadata.extensions) ? { ...metadata.extensions } : {}
    extensions.at_exponential_theme = body.theme
    metadata.extensions = extensions
    out.metadata = metadata
  }
  return out
}

function upgradeUpdateComponents(body: Obj, state: Upgrade09State, ctx: Ctx): Obj {
  const surfaceId = typeof body.surfaceId === `string` ? body.surfaceId : ``
  const out: Obj = {}
  for (const [k, v] of Object.entries(body)) out[k] = k === `components` ? upgradeComponentsWith(v, state.catalogs[surfaceId], state, surfaceId, ctx) : v
  return out
}

/** §14.2: one 0.9 agent → renderer (or host-internal) message → exactly one
 *  1.0 message, threading the id maps of a stream through `state` (copied,
 *  never mutated). */
export function upgradeMessage09(message: unknown, state?: Upgrade09State): { messages: unknown[]; state: Upgrade09State } {
  const next = cloneState(state)
  if (!isObj(message)) return { messages: [message], state: next }
  const ctx: Ctx = { tools: {} }
  const rest: Obj = {}
  for (const [k, v] of Object.entries(message)) {
    if (k === `version`) continue
    if (k === `createSurface` && isObj(v)) rest[k] = upgradeCreateSurface(v, next, ctx)
    else if (k === `updateComponents` && isObj(v)) rest[k] = upgradeUpdateComponents(v, next, ctx)
    else if (k === `updateDataModel` && isObj(v)) rest[k] = own(v, `value`) ? v : { ...v, value: null }
    else rest[k] = v
  }
  return { messages: [withVersion(rest)], state: next }
}

const CLIENT_CODES: Record<string, string> = {
  FUNCTION_NOT_FOUND: `UNKNOWN_FUNCTION`,
  FUNCTION_DENIED: `INVALID_FUNCTION_CALL`,
  ACTION_FAILED: `EXECUTION_ERROR`,
}

function upgradeClientAction(body: Obj): Obj {
  const out: Obj = {}
  let payload: unknown
  let instance: string | undefined
  for (const [k, v] of Object.entries(body)) {
    if (k === `payload`) {
      if (present(v) && !(isObj(v) && Object.keys(v).length === 0)) payload = v
      continue
    }
    if (k === `sourceComponentId` && typeof v === `string`) {
      const dot = v.indexOf(`.`)
      const authored = mangle(dot < 0 ? v : v.slice(0, dot))
      out[k] = authored
      if (authored !== v) instance = v
      continue
    }
    out[k] = v
  }
  if (payload !== undefined || instance !== undefined) {
    // §1.3 order: at_exponential_payload, then at_exponential_instance.
    const metadata: Obj = isObj(out.metadata) ? { ...out.metadata } : {}
    const extensions: Obj = isObj(metadata.extensions) ? { ...metadata.extensions } : {}
    if (payload !== undefined) extensions.at_exponential_payload = payload
    if (instance !== undefined) extensions.at_exponential_instance = instance
    metadata.extensions = extensions
    out.metadata = metadata
  }
  return out
}

/** §14.2: a 0.9 renderer → agent message (`action`, `error`) → 1.0. */
export function upgradeClientMessage09(message: unknown): unknown {
  if (!isObj(message)) return message
  const rest: Obj = {}
  for (const [k, v] of Object.entries(message)) {
    if (k === `version`) continue
    if (k === `action` && isObj(v)) rest[k] = upgradeClientAction(v)
    else if (k === `error` && isObj(v)) rest[k] = typeof v.code === `string` && own(CLIENT_CODES, v.code) ? { ...v, code: CLIENT_CODES[v.code] } : v
    else rest[k] = v
  }
  return withVersion(rest)
}

// ---------------------------------------------------------------------------
// Cards (nested {root, data})

function collectCardIds(node: unknown, out: string[]): void {
  if (!isObj(node)) return
  if (typeof node.id === `string`) out.push(node.id)
  if (isObj(node.slots)) for (const k of Object.keys(node.slots).sort()) collectCardIds(node.slots[k], out)
  if (Array.isArray(node.children)) for (const c of node.children) collectCardIds(c, out)
}

function upgradeCardNode(node: unknown, rekey: Rekey, ctx: Ctx): unknown {
  if (!isObj(node)) return node
  const out: Obj = {}
  for (const [k, v] of Object.entries(node)) {
    switch (k) {
      case `id`:
        out.id = rekey.ref(v)
        break
      case `props`:
        if (isObj(v)) {
          const props: Obj = {}
          for (const [pk, pv] of Object.entries(v)) props[pk] = upgradeProp(pk, pv, node.component, false, rekey, ctx)
          out[k] = props
        } else out[k] = v
        break
      case `children`:
        out[k] = Array.isArray(v) ? v.map((c) => upgradeCardNode(c, rekey, ctx)) : v
        break
      case `slots`:
        if (isObj(v)) {
          const slots: Obj = {}
          for (const [sk, sv] of Object.entries(v)) slots[sk] = upgradeCardNode(sv, rekey, ctx)
          out[k] = slots
        } else out[k] = v
        break
      case `template`:
        out[k] = rekeyTemplate(v, rekey)
        break
      case `visible`:
        out[k] = asPresence(dyn(v, ctx), `visible`)
        break
      case `accessibility`:
        out[k] = upgradeAccessibility(v, ctx)
        break
      case `on`:
        out[k] = upgradeProp(k, v, node.component, false, rekey, ctx)
        break
      default:
        out[k] = v
    }
  }
  return out
}

/** §14.2: a nested card `{root, data}` → 1.0, `"version": "1.0"` first. */
export function upgradeCard09(surface: unknown): unknown {
  if (!isObj(surface)) return surface
  const state = emptyUpgrade09State()
  const rekey = Rekey.forSurface(state, ``)
  const ids: string[] = []
  collectCardIds(surface.root, ids)
  rekey.define(ids)
  const ctx: Ctx = { tools: {} }
  const rest: Obj = {}
  for (const [k, v] of Object.entries(surface)) {
    if (k === `version`) continue
    rest[k] = k === `root` ? upgradeCardNode(v, rekey, ctx) : v
  }
  return { version: `1.0`, ...rest }
}

// ---------------------------------------------------------------------------
// Packages (formatVersion 5 → 6)

const TOOL_ID_V6 = /^[a-z][a-z0-9]*(_[a-z0-9]+)*$/
const FUNCTION_NAME_09 = /^[A-Za-z_][A-Za-z0-9_-]*(\.[A-Za-z0-9_-]+)+$/

function toolBase(id: string): string {
  const flat = mangle(id).replace(/_+/g, `_`).replace(/^_+|_+$/g, ``)
  return flat === `` ? `_` : flat
}

function rekeyTools(ids: readonly string[]): ToolMap {
  const map: ToolMap = {}
  const kept = new Set(ids.filter((id) => TOOL_ID_V6.test(id)))
  const taken = new Set<string>(kept)
  for (const id of ids) {
    if (kept.has(id)) {
      map[id] = id
      continue
    }
    const base = toolBase(id)
    let target = base
    for (let n = 2; taken.has(target); n++) target = `${base}_${n}`
    map[id] = target
    taken.add(target)
  }
  return map
}

function renameExpect(value: unknown, ctx: Ctx): unknown {
  if (typeof value === `string`) return FUNCTION_NAME_09.test(value) ? renameWith(value, ctx.tools) : value
  if (Array.isArray(value)) return value.map((v) => renameExpect(v, ctx))
  if (isObj(value)) {
    const out: Obj = {}
    for (const [k, v] of Object.entries(value)) out[k] = renameExpect(v, ctx)
    return out
  }
  return value
}

const STEP_KEYS: Record<string, string> = { "records.upsert": `records_upsert`, "records.delete": `records_delete` }

function upgradeActions(actions: unknown, ctx: Ctx): unknown {
  if (!isObj(actions)) return actions
  const out: Obj = {}
  for (const [name, action] of Object.entries(actions)) {
    if (!isObj(action) || !Array.isArray(action.steps)) {
      out[name] = action
      continue
    }
    const next: Obj = {}
    for (const [k, v] of Object.entries(action)) {
      next[k] =
        k === `steps`
          ? (v as unknown[]).map((step) => {
              if (!isObj(step)) return step
              const s: Obj = {}
              for (const [sk, sv] of Object.entries(step)) {
                if (sk === `call`) s[sk] = typeof sv === `string` ? renameWith(sv, ctx.tools) : sv
                else s[own(STEP_KEYS, sk) ? STEP_KEYS[sk]! : sk] = sv
              }
              return s
            })
          : v
    }
    out[name] = next
  }
  return out
}

function upgradeExposes(exposes: unknown, ctx: Ctx): unknown {
  if (!Array.isArray(exposes)) return exposes
  return exposes.map((expose) => {
    if (!isObj(expose) || !isObj(expose.run) || typeof expose.run.action !== `string`) return expose
    const out: Obj = {}
    for (const [k, v] of Object.entries(expose)) out[k] = k === `run` ? { ...(v as Obj), action: renameWith((v as Obj).action as string, ctx.tools) } : v
    return out
  })
}

/** §14.3 Packages, steps 2–5, over a package OBJECT (no signature
 *  handling: `signature` is dropped, `author` kept). A package that does
 *  not name format 5 is returned unchanged. */
export function upgradePackageBody09(pkg: unknown): unknown {
  if (!isObj(pkg) || pkg.formatVersion !== 5) return pkg
  const tools = isObj(pkg.tools) ? rekeyTools(Object.keys(pkg.tools)) : {}
  const ctx: Ctx = { tools }
  const catalogId = upgradeCatalogId(pkg.catalogId)
  const out: Obj = {}
  for (const [k, v] of Object.entries(pkg)) {
    switch (k) {
      case `signature`:
        break
      case `formatVersion`:
        out[k] = 6
        break
      case `catalogId`:
        out[k] = catalogId
        break
      case `templates`:
        if (isObj(v)) {
          const templates: Obj = {}
          for (const [id, template] of Object.entries(v)) {
            if (!isObj(template)) {
              templates[id] = template
              continue
            }
            const state = emptyUpgrade09State()
            const t: Obj = {}
            for (const [tk, tv] of Object.entries(template)) t[tk] = tk === `components` ? upgradeComponentsWith(tv, catalogId, state, ``, ctx) : tv
            templates[id] = t
          }
          out[k] = templates
        } else out[k] = v
        break
      case `functions`:
        out[k] = Array.isArray(v) ? v.map((p) => (typeof p === `string` ? renamePatternWith(p, tools) : p)) : v
        break
      case `actions`:
        out[k] = upgradeActions(v, ctx)
        break
      case `exposes`:
        out[k] = upgradeExposes(v, ctx)
        break
      case `tests`:
        out[k] = Array.isArray(v) ? v.map((t) => (isObj(t) && own(t, `expect`) ? { ...t, expect: renameExpect(t.expect, ctx) } : t)) : v
        break
      case `tools`:
        if (isObj(v)) {
          const re: Obj = {}
          for (const [id, tool] of Object.entries(v)) re[tools[id]!] = tool
          out[k] = re
        } else out[k] = v
        break
      default:
        out[k] = v
    }
  }
  return out
}

export interface Upgrade09Package {
  package: unknown
  status: `unsigned` | `valid` | `invalid`
  author?: unknown
}

/** §14.3 Packages: `verify` runs on the ORIGINAL file; `invalid` → no
 *  package; else the upgraded body with the original's status (`valid`
 *  keeps the verified author). */
export async function upgradePackage09(file: unknown, verify: (file: unknown) => Promise<PackageFileVerification> = verifyPackageFile): Promise<Upgrade09Package> {
  const verdict = await verify(file)
  if (verdict.status === `invalid`) return { package: null, status: `invalid` }
  const pkg = upgradePackageBody09(file)
  return verdict.status === `valid` ? { package: pkg, status: `valid`, author: verdict.author } : { package: pkg, status: `unsigned` }
}
