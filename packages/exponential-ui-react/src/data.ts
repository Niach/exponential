// VAPP-87: the surface DATA MODEL and A2UI's dynamic values. Props may carry
// a `{path}` binding (a JSON Pointer into the data model, relative inside a
// template item) or a `{call, args}` function call (the basic catalog's 14
// client functions + round 1's 13 CORE functions the macro expander emits
// for bound inputs); `resolveValue` turns a prop tree into literals for one
// render, `$string.<id>` built-in copy included (round-1 contract §1, §4:
// the React mirror of `@exponential-at/ui` `resolveDynamic`). Formatting
// runs in the SURFACE locale. Pure, so the host can run it too.

import { CORE_FUNCTIONS, DEFAULT_LOCALE, resolveString, truthy } from "@exponential-at/ui"

export type DataModel = Record<string, unknown> | unknown[]

export interface DataBinding {
  path: string
}
export interface FunctionCall {
  call: string
  args?: Record<string, unknown>
  returnType?: string
}

export function isBinding(value: unknown): value is DataBinding {
  return typeof value === `object` && value !== null && !Array.isArray(value) && typeof (value as DataBinding).path === `string` && Object.keys(value as object).length === 1
}
export function isCall(value: unknown): value is FunctionCall {
  return typeof value === `object` && value !== null && !Array.isArray(value) && typeof (value as FunctionCall).call === `string`
}

const unescape = (token: string) => token.replace(/~1/g, `/`).replace(/~0/g, `~`)

/** A pointer's tokens; `` and `/` are the whole model. */
export function pointerTokens(pointer: string): string[] {
  if (pointer === `` || pointer === `/`) return []
  return (pointer.startsWith(`/`) ? pointer.slice(1) : pointer).split(`/`).map(unescape)
}

/** `path` made absolute against a template scope (`/items/3`). */
export function absolutePath(path: string, scope = ``): string {
  if (path.startsWith(`/`)) return path
  if (!scope) return `/${path}`
  return path === `` ? scope : `${scope}/${path}`
}

export function getPointer(data: unknown, pointer: string): unknown {
  let cur: unknown = data
  for (const token of pointerTokens(pointer)) {
    if (cur === null || typeof cur !== `object`) return undefined
    cur = Array.isArray(cur) ? cur[Number(token)] : (cur as Record<string, unknown>)[token]
  }
  return cur
}

/** A copy of `data` with the value at `pointer` replaced (or removed when
 *  `value` is undefined); intermediate objects are created. */
export function setPointer<T extends DataModel>(data: T, pointer: string, value: unknown): T {
  const tokens = pointerTokens(pointer)
  if (tokens.length === 0) return (value === undefined ? {} : value) as T
  const put = (cur: unknown, i: number): unknown => {
    const token = tokens[i]
    const last = i === tokens.length - 1
    const base: unknown = cur !== null && typeof cur === `object` ? cur : /^\d+$/.test(token) ? [] : {}
    if (Array.isArray(base)) {
      const next = [...base]
      const idx = token === `-` ? next.length : Number(token)
      if (last) {
        if (value === undefined) next.splice(idx, 1)
        else next[idx] = value
      } else next[idx] = put(next[idx], i + 1)
      return next
    }
    const next = { ...(base as Record<string, unknown>) }
    if (last) {
      if (value === undefined) delete next[token]
      else next[token] = value
    } else next[token] = put(next[token], i + 1)
    return next
  }
  return put(data, 0) as T
}

// ---------------------------------------------------------------------------
// Client functions
// ---------------------------------------------------------------------------

export interface ResolveContext {
  data: unknown
  /** The template item's pointer, for relative paths. */
  scope?: string
  functions?: Record<string, ClientFunction>
  /** `openUrl` and other side effects go to the host. */
  openUrl?: (url: string) => void
  /** The surface locale (BCP 47) the format functions use; default en-US. */
  locale?: string
  /** The surface's built-in string table: `$string.<id>` resolves through it. */
  strings?: Readonly<Record<string, string>>
}

/** The synthetic root a Table's LITERAL rows are mounted under in a slot
 *  cell's read view: never written to the real data model (the table keeps
 *  those edits), never reported to the host as a path. */
export const LITERAL_ROWS_ROOT = `/$xuiRows`

/** A bound path the host may see (`undefined` for a synthetic one). */
export function hostPath(path: string | undefined): string | undefined {
  return path === undefined || path === LITERAL_ROWS_ROOT || path.startsWith(`${LITERAL_ROWS_ROOT}/`) ? undefined : path
}

export type ClientFunction = (args: Record<string, unknown>, ctx: ResolveContext) => unknown

const num = (v: unknown) => (typeof v === `number` ? v : Number(v))

/** `${/path}` and `${rel/path}` interpolations (formatString). */
function interpolate(text: string, ctx: ResolveContext): string {
  return text.replace(/\$\{([^}]+)\}/g, (_, expr: string) => {
    const v = getPointer(ctx.data, absolutePath(expr.trim(), ctx.scope))
    return v === undefined || v === null ? `` : String(v)
  })
}

function pad(pattern: string, date: Date, locale: string): string {
  const map: Record<string, string> = {
    yyyy: String(date.getFullYear()),
    yy: String(date.getFullYear()).slice(-2),
    MMMM: date.toLocaleString(locale, { month: `long` }),
    MMM: date.toLocaleString(locale, { month: `short` }),
    MM: String(date.getMonth() + 1).padStart(2, `0`),
    M: String(date.getMonth() + 1),
    dd: String(date.getDate()).padStart(2, `0`),
    d: String(date.getDate()),
    EEEE: date.toLocaleString(locale, { weekday: `long` }),
    EEE: date.toLocaleString(locale, { weekday: `short` }),
    HH: String(date.getHours()).padStart(2, `0`),
    H: String(date.getHours()),
    hh: String(date.getHours() % 12 || 12).padStart(2, `0`),
    h: String(date.getHours() % 12 || 12),
    mm: String(date.getMinutes()).padStart(2, `0`),
    ss: String(date.getSeconds()).padStart(2, `0`),
    a: date.getHours() < 12 ? `AM` : `PM`,
  }
  return pattern.replace(/yyyy|yy|MMMM|MMM|MM|M|dd|d|EEEE|EEE|HH|H|hh|h|mm|ss|a/g, (t) => map[t] ?? t)
}

const loc = (ctx: ResolveContext) => ctx.locale ?? DEFAULT_LOCALE

/** The core functions (percent, add, sub, eq, lt, cond, fallback, concat,
 *  coalesce, text, map, len) exactly as the catalog evaluates them. `set` is
 *  an ACTION (`node-view.tsx` runs it), never a value. */
const CORE: Record<string, ClientFunction> = Object.fromEntries(Object.entries(CORE_FUNCTIONS).map(([name, fn]) => [name, (args: Record<string, unknown>) => fn(args)]))

export const CLIENT_FUNCTIONS: Record<string, ClientFunction> = {
  // The contract's `required` (dynamic.ts LOGIC_FUNCTIONS, Rust data.rs):
  // truthy (so `false` and `""` fail, 0 passes) and not an empty array.
  required: ({ value }) => truthy(value) && !(Array.isArray(value) && value.length === 0),
  regex: ({ value, pattern }) => {
    try {
      return new RegExp(String(pattern)).test(String(value ?? ``))
    } catch {
      return false
    }
  },
  length: ({ value, min, max }) => {
    const n = String(value ?? ``).length
    return (min === undefined || n >= num(min)) && (max === undefined || n <= num(max))
  },
  numeric: ({ value, min, max }) => {
    const n = num(value)
    return Number.isFinite(n) && (min === undefined || n >= num(min)) && (max === undefined || n <= num(max))
  },
  email: ({ value }) => /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(String(value ?? ``)),
  formatString: ({ value }, ctx) => interpolate(String(value ?? ``), ctx),
  formatNumber: ({ value, decimals }, ctx) => {
    const n = num(value)
    return Number.isFinite(n) ? n.toLocaleString(loc(ctx), { minimumFractionDigits: decimals === undefined ? 0 : num(decimals), maximumFractionDigits: decimals === undefined ? 2 : num(decimals) }) : ``
  },
  formatCurrency: ({ value, currency, decimals }, ctx) => {
    const n = num(value)
    try {
      return n.toLocaleString(loc(ctx), { style: `currency`, currency: String(currency ?? `USD`), minimumFractionDigits: decimals === undefined ? undefined : num(decimals), maximumFractionDigits: decimals === undefined ? undefined : num(decimals) })
    } catch {
      return `${currency ?? ``} ${n}`
    }
  },
  formatDate: ({ value, format }, ctx) => {
    const date = value instanceof Date ? value : new Date(String(value))
    if (Number.isNaN(date.getTime())) return ``
    return format ? pad(String(format), date, loc(ctx)) : date.toLocaleDateString(loc(ctx))
  },
  pluralize: ({ value, zero, one, two, few, many, other }, ctx) => {
    const n = num(value)
    let category: string
    try {
      category = new Intl.PluralRules(loc(ctx)).select(n)
    } catch {
      category = n === 1 ? `one` : `other`
    }
    if (n === 0 && zero !== undefined) return zero
    const forms: Record<string, unknown> = { zero, one, two, few, many, other }
    return forms[category] ?? other ?? ``
  },
  openUrl: ({ url }, ctx) => {
    if (typeof url === `string`) ctx.openUrl?.(url)
    return undefined
  },
  // Truthiness = the catalog's `truthy` (0 is TRUE, "" is false).
  and: ({ values }) => (Array.isArray(values) ? values.every(truthy) : false),
  or: ({ values }) => (Array.isArray(values) ? values.some(truthy) : false),
  not: ({ value }) => !truthy(value),
  ...CORE,
}

/** A prop value with its bindings and calls resolved for one render. Plain
 *  objects recurse (a check's `condition`, a menu item's label). */
export function resolveValue(value: unknown, ctx: ResolveContext): unknown {
  if (isBinding(value)) return getPointer(ctx.data, absolutePath(value.path, ctx.scope))
  if (isCall(value)) {
    const fn = ctx.functions?.[value.call] ?? CLIENT_FUNCTIONS[value.call]
    if (!fn) return undefined
    const args: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(value.args ?? {})) args[k] = resolveValue(v, ctx)
    return fn(args, ctx)
  }
  if (typeof value === `string`) return ctx.strings && value.startsWith(`$string.`) ? resolveString(value, ctx.strings) : value
  if (Array.isArray(value)) return value.map((item) => resolveValue(item, ctx))
  if (typeof value === `object` && value !== null) {
    const out: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(value)) out[k] = resolveValue(v, ctx)
    return out
  }
  return value
}

export function resolveProps(props: Record<string, unknown>, ctx: ResolveContext): Record<string, unknown> {
  return resolveValue(props, ctx) as Record<string, unknown>
}

/** The pointer a bindable prop writes back to (two-way state), if bound. */
export function boundPath(props: Record<string, unknown>, name: string, scope = ``): string | undefined {
  const raw = props[name]
  return isBinding(raw) ? absolutePath(raw.path, scope) : undefined
}

/** A node's `visible`: absent = shown, else its resolved truthiness. */
export function resolveVisible(visible: unknown, ctx: ResolveContext): boolean {
  if (visible === undefined) return true
  return truthy(resolveValue(visible, ctx))
}

/** True when a value holds a binding or a call anywhere (a style value the
 *  expander left dynamic). */
export function hasDynamic(value: unknown): boolean {
  if (isBinding(value) || isCall(value)) return true
  if (Array.isArray(value)) return value.some(hasDynamic)
  if (typeof value === `object` && value !== null) return Object.values(value).some(hasDynamic)
  return false
}

/** The React renderer's BIND pass as a pure function (round-1 contract §1;
 *  `@exponential-at/ui` `bindTree` is the reference, fixtures/bind-time.json
 *  locks both): a node whose `visible` resolves falsy is dropped with its
 *  subtree, props / style values / recipe props resolve, `visible` goes,
 *  actions and templates stay. `NodeView` binds node by node with the same
 *  `resolveValue`; this is the whole-tree form for tests and hosts. */
/** A node's `accessibility` resolved against the data (bindings, calls,
 *  `$string`), undefined members dropped; null when nothing is left. */
export function resolveAccessibility(accessibility: unknown, ctx: ResolveContext): Record<string, unknown> | null {
  if (!accessibility || typeof accessibility !== `object`) return null
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(resolveValue(accessibility, ctx) as Record<string, unknown>)) if (v !== undefined) out[k] = v
  return Object.keys(out).length ? out : null
}

/** The ARIA attributes of a RESOLVED `accessibility` (contract §6): `role`
 *  (`text` = none of its own, `hidden` = aria-hidden), label/description,
 *  `level`, `expanded`, `current` (page|step; false = absent), `selected`,
 *  `pressed`, `checked`, `valueNow/Min/Max`, `hidden`; `autoFocus` →
 *  `data-xui-autofocus` (an opening dialog focuses it). */
export function ariaAttributes(a11y: Record<string, unknown> | null): Record<string, string> {
  const out: Record<string, string> = {}
  if (!a11y) return out
  const role = a11y.role
  if (role === `hidden`) out[`aria-hidden`] = `true`
  else if (typeof role === `string` && role !== `text` && role !== ``) out.role = role
  if (a11y.label !== undefined && a11y.label !== null && a11y.label !== ``) out[`aria-label`] = String(a11y.label)
  if (a11y.description !== undefined && a11y.description !== null && a11y.description !== ``) out[`aria-description`] = String(a11y.description)
  if (typeof a11y.level === `number` || (typeof a11y.level === `string` && a11y.level !== ``)) out[`aria-level`] = String(a11y.level)
  for (const [key, attr] of [[`expanded`, `aria-expanded`], [`selected`, `aria-selected`], [`pressed`, `aria-pressed`], [`checked`, `aria-checked`]] as const) {
    const v = a11y[key]
    if (typeof v === `boolean` || v === `mixed`) out[attr] = String(v)
  }
  if (a11y.current !== undefined && a11y.current !== null && a11y.current !== false && a11y.current !== ``) out[`aria-current`] = a11y.current === true ? `true` : String(a11y.current)
  for (const [key, attr] of [[`valueNow`, `aria-valuenow`], [`valueMin`, `aria-valuemin`], [`valueMax`, `aria-valuemax`]] as const) {
    const v = a11y[key]
    if (typeof v === `number` && Number.isFinite(v)) out[attr] = String(v)
  }
  if (a11y.hidden === true) out[`aria-hidden`] = `true`
  if (a11y.autoFocus === true) out[`data-xui-autofocus`] = ``
  return out
}

export function bindTree<T extends { id: string; component: string; props: Record<string, unknown>; children: T[]; style?: Record<string, unknown>; visible?: unknown; slots?: Record<string, T>; recipe?: { macro: string; part: string; props: Record<string, unknown> }; on?: unknown; accessibility?: unknown; template?: unknown }>(node: T, ctx: ResolveContext): T | null {
  if (!resolveVisible(node.visible, ctx)) return null
  const out = { id: node.id, component: node.component, props: resolveProps(node.props, ctx), children: [] as T[] } as unknown as T
  if (node.style) out.style = resolveValue(node.style, ctx) as Record<string, unknown>
  if (node.on) out.on = node.on
  if (node.accessibility) {
    // Contract §1/§6: accessibility (label, description and the macro
    // part's `$a11y` states) resolves like props; an empty result is dropped.
    const a11y = resolveAccessibility(node.accessibility, ctx)
    if (a11y) out.accessibility = a11y
  }
  for (const child of node.children) {
    const bound = bindTree(child, ctx)
    if (bound) out.children.push(bound)
  }
  if (node.slots) {
    for (const [name, slot] of Object.entries(node.slots)) {
      const bound = bindTree(slot, ctx)
      if (bound) (out.slots ??= {} as Record<string, T>)[name] = bound
    }
  }
  if (node.template) out.template = node.template
  if (node.recipe) out.recipe = { ...node.recipe, props: resolveValue(node.recipe.props, ctx) as Record<string, unknown> }
  return out
}
