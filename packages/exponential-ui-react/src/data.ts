// VAPP-87: the surface DATA MODEL and A2UI's dynamic values. Props may carry
// a `{path}` binding (a JSON Pointer into the data model, relative inside a
// template item) or a `{call, args}` function call (the catalog's 14
// client functions); `resolveValue` turns a prop tree into literals for one
// render. Pure, so the host can run it too (the Rust core mirrors it).

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
}

export type ClientFunction = (args: Record<string, unknown>, ctx: ResolveContext) => unknown

const empty = (v: unknown) => v === undefined || v === null || v === `` || (Array.isArray(v) && v.length === 0)
const num = (v: unknown) => (typeof v === `number` ? v : Number(v))

/** `${/path}` and `${rel/path}` interpolations (formatString). */
function interpolate(text: string, ctx: ResolveContext): string {
  return text.replace(/\$\{([^}]+)\}/g, (_, expr: string) => {
    const v = getPointer(ctx.data, absolutePath(expr.trim(), ctx.scope))
    return v === undefined || v === null ? `` : String(v)
  })
}

function pad(pattern: string, date: Date): string {
  const map: Record<string, string> = {
    yyyy: String(date.getFullYear()),
    yy: String(date.getFullYear()).slice(-2),
    MMMM: date.toLocaleString(undefined, { month: `long` }),
    MMM: date.toLocaleString(undefined, { month: `short` }),
    MM: String(date.getMonth() + 1).padStart(2, `0`),
    M: String(date.getMonth() + 1),
    dd: String(date.getDate()).padStart(2, `0`),
    d: String(date.getDate()),
    EEEE: date.toLocaleString(undefined, { weekday: `long` }),
    EEE: date.toLocaleString(undefined, { weekday: `short` }),
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

export const CLIENT_FUNCTIONS: Record<string, ClientFunction> = {
  required: ({ value }) => !empty(value),
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
  formatNumber: ({ value, decimals }) => {
    const n = num(value)
    return Number.isFinite(n) ? n.toLocaleString(undefined, { minimumFractionDigits: decimals === undefined ? 0 : num(decimals), maximumFractionDigits: decimals === undefined ? 2 : num(decimals) }) : ``
  },
  formatCurrency: ({ value, currency, decimals }) => {
    const n = num(value)
    try {
      return n.toLocaleString(undefined, { style: `currency`, currency: String(currency ?? `USD`), minimumFractionDigits: decimals === undefined ? undefined : num(decimals), maximumFractionDigits: decimals === undefined ? undefined : num(decimals) })
    } catch {
      return `${currency ?? ``} ${n}`
    }
  },
  formatDate: ({ value, format }) => {
    const date = value instanceof Date ? value : new Date(String(value))
    if (Number.isNaN(date.getTime())) return ``
    return format ? pad(String(format), date) : date.toLocaleDateString()
  },
  pluralize: ({ value, zero, one, two, few, many, other }) => {
    const n = num(value)
    let category: string
    try {
      category = new Intl.PluralRules().select(n)
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
  and: ({ values }) => (Array.isArray(values) ? values.every(Boolean) : false),
  or: ({ values }) => (Array.isArray(values) ? values.some(Boolean) : false),
  not: ({ value }) => !value,
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
