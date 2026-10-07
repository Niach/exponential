// VAPP-85: the macro template's value language (catalog/macros.json
// `$comment`): `{props.x}` paths with `|fallback`, `len(path)`, `!path`,
// string interpolation, and the $map / $cond / $eq / $add / $percent /
// $coalesce value objects. Pure, so the Rust core can mirror it line by line.

export interface ExprContext {
  id: string
  props: Record<string, unknown>
  vars: Record<string, unknown>
}

const WHOLE = /^\{([^{}]+)\}$/
const EMBEDDED = /\{([^{}]+)\}/g

/** `undefined`, `null`, `false` and `""` are false; 0 is TRUE (a count of
 *  zero still shows). */
export function truthy(value: unknown): boolean {
  return value !== undefined && value !== null && value !== false && value !== ``
}

function parseLiteral(text: string): unknown {
  if (text === `true`) return true
  if (text === `false`) return false
  if (text === `null`) return null
  if (/^-?\d+(\.\d+)?$/.test(text)) return Number(text)
  return text
}

function lookup(path: string, ctx: ExprContext): unknown {
  if (path === `id`) return ctx.id
  const [head, ...rest] = path.split(`.`)
  let cur: unknown
  if (head === `props`) cur = ctx.props
  else if (head in ctx.vars) cur = ctx.vars[head]
  else return undefined
  for (const key of rest) {
    if (cur === undefined || cur === null) return undefined
    cur = (cur as Record<string, unknown>)[key]
  }
  return cur
}

/** One expression: `path`, `!path`, `len(path)`, `path|fallback`. */
export function evalExpr(expr: string, ctx: ExprContext): unknown {
  const text = expr.trim()
  const bar = text.indexOf(`|`)
  if (bar >= 0) {
    const value = evalExpr(text.slice(0, bar), ctx)
    return value === undefined || value === null ? parseLiteral(text.slice(bar + 1)) : value
  }
  if (text.startsWith(`!`)) return !truthy(evalExpr(text.slice(1), ctx))
  const len = /^len\((.+)\)$/.exec(text)
  if (len) {
    const value = evalExpr(len[1], ctx)
    return Array.isArray(value) ? value.length : typeof value === `string` ? value.length : 0
  }
  return lookup(text, ctx)
}

/** A condition: a string expression or a value object ($eq …). */
export function evalCondition(cond: unknown, ctx: ExprContext): boolean {
  if (typeof cond === `string`) return truthy(evalExpr(cond, ctx))
  return truthy(evalValue(cond, ctx))
}

function toNumber(value: unknown): number {
  return typeof value === `number` ? value : Number(value ?? 0) || 0
}

/** Any template value: strings interpolate, arrays and objects recurse, the
 *  `$`-objects compute. `undefined` results are dropped by the caller. */
export function evalValue(value: unknown, ctx: ExprContext): unknown {
  if (typeof value === `string`) {
    const whole = WHOLE.exec(value)
    if (whole) return evalExpr(whole[1], ctx)
    return value.replace(EMBEDDED, (_, expr: string) => {
      const v = evalExpr(expr, ctx)
      return v === undefined || v === null ? `` : String(v)
    })
  }
  if (Array.isArray(value)) {
    return value.map((item) => evalValue(item, ctx)).filter((item) => item !== undefined)
  }
  if (typeof value !== `object` || value === null) return value
  const obj = value as Record<string, unknown>
  if (`$map` in obj) {
    const spec = obj.$map as { from: string; cases: Record<string, unknown>; default?: unknown }
    const from = evalExpr(spec.from, ctx)
    const key = from === undefined || from === null ? `` : String(from)
    const hit = Object.prototype.hasOwnProperty.call(spec.cases, key) ? spec.cases[key] : spec.default
    return evalValue(hit, ctx)
  }
  if (`$cond` in obj) {
    const [cond, then, otherwise] = obj.$cond as [unknown, unknown, unknown]
    return evalValue(evalCondition(cond, ctx) ? then : otherwise, ctx)
  }
  if (`$eq` in obj) {
    const [a, b] = (obj.$eq as [unknown, unknown]).map((x) => evalValue(x, ctx))
    return a === b
  }
  if (`$add` in obj) {
    const [a, b] = (obj.$add as [unknown, unknown]).map((x) => evalValue(x, ctx))
    return toNumber(a) + toNumber(b)
  }
  if (`$percent` in obj) {
    const [v, max] = (obj.$percent as [unknown, unknown]).map((x) => evalValue(x, ctx))
    const m = toNumber(max)
    const pct = m > 0 ? (toNumber(v) / m) * 100 : 0
    const clamped = Math.min(100, Math.max(0, pct))
    return `${Math.round(clamped * 100) / 100}%`
  }
  if (`$text` in obj) {
    const v = evalExpr(obj.$text as string, ctx)
    return v === undefined || v === null ? undefined : String(v)
  }
  if (`$coalesce` in obj) {
    for (const candidate of obj.$coalesce as unknown[]) {
      const v = evalValue(candidate, ctx)
      if (truthy(v)) return v
    }
    return undefined
  }
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(obj)) {
    const evaluated = evalValue(v, ctx)
    if (evaluated !== undefined) out[k] = evaluated
  }
  return out
}
