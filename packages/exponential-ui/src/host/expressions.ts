// VAPP-191 (eApps Q2): expressions are type-checked at package validation.
// Every `{"call": "expr", "args": {"expr": "<CEL>", "vars": {…}}}` anywhere
// in a template is parsed and checked (src/cel.ts) against the types of its
// vars: a literal's own type, a records binding's value (the kind's schema
// fields) or the template data's; anything else is open (`dyn`). An issue
// names the JSON pointer of the expression and the reason, so the editor
// agent can fix it. The Rust core's `host::expressions` mirrors it
// (`fixtures/expressions.json` `packages`).

import { checkExpression, optionalType, typeName } from "../cel"
import type { ExprType } from "../cel"
import { isBinding, isCall } from "../expr"
import { RECORDS_SCHEME } from "./query"
import { parseSource } from "./sources"

export interface ExpressionIssue {
  path: string
  message: string
}

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)
const escape = (key: string) => key.replace(/~/g, `~0`).replace(/\//g, `~1`)
const own = (o: Record<string, unknown>, k: string) => Object.prototype.hasOwnProperty.call(o, k)

/** A JSON Schema property → its expression type. */
export function schemaType(schema: unknown): ExprType {
  if (!isObject(schema)) return `dyn`
  const types = Array.isArray(schema.type) ? schema.type.filter((t) => t !== `null`) : [schema.type]
  const type = types.length === 1 ? types[0] : undefined
  if (type === `number` || type === `integer`) return `number`
  if (type === `boolean`) return `bool`
  if (type === `string`) return schema.format === `date` ? `date` : schema.format === `date-time` ? `timestamp` : `string`
  if (type === `array`) return [schemaType(schema.items)]
  if (type === `object`) return isObject(schema.properties) ? rowOf(schema.properties, schema.required) : `map`
  return `dyn`
}

/** VAPP-195 (E2): a schema's fields; one not in `required` may be missing
 *  (`T?`). */
export function rowOf(properties: Record<string, unknown>, required: unknown): ExprType {
  const names = Array.isArray(required) ? required : []
  const out: Record<string, ExprType> = {}
  for (const [k, v] of Object.entries(properties)) out[k] = names.includes(k) ? schemaType(v) : optionalType(schemaType(v))
  return out
}

/** A record row of a kind: its schema's fields (`T?` unless required) plus
 *  the row keys (always there). */
export function recordRowType(kind: unknown): ExprType {
  const schema = isObject(kind) && isObject(kind.schema) ? kind.schema : {}
  const props = isObject(schema.properties) ? schema.properties : {}
  return { ...(rowOf(props, schema.required) as Record<string, ExprType>), id: `string`, createdAt: `timestamp`, updatedAt: `timestamp`, createdBy: `string`, authorId: `string`, author: `string` }
}

/** The value a records binding lands (docs/package-format.md "Records"). */
export function recordsValueType(kind: unknown): ExprType {
  return { rows: [recordRowType(kind)], count: `number`, sum: `number`, groups: [`map`], totals: `map`, before: `map`, month: `map` }
}

/** A literal's type: scalars, lists (one item type, else dyn), rows. */
export function literalType(v: unknown): ExprType {
  if (Array.isArray(v)) {
    const items = v.map(literalType)
    if (items.length === 0) return [`dyn`]
    return [items.every((t) => JSON.stringify(t) === JSON.stringify(items[0])) ? items[0]! : `dyn`]
  }
  if (isObject(v)) {
    const out: Record<string, ExprType> = {}
    for (const [k, x] of Object.entries(v)) out[k] = literalType(x)
    return out
  }
  return typeName(v)
}

/** The type at the rest of a pointer below a type. */
function walk(type: ExprType, segments: string[]): ExprType {
  let t = type
  for (const seg of segments) {
    if (Array.isArray(t)) {
      if (!/^\d+$/.test(seg)) return `dyn`
      t = t[0]!
    } else if (typeof t === `object`) {
      if (!own(t, seg)) return `dyn`
      t = t[seg]!
    } else return `dyn`
  }
  return t
}

const unescape = (s: string) => s.replace(/~1/g, `/`).replace(/~0/g, `~`)

/** The type of an absolute data path in a template: below a records
 *  binding its value, else the template data's literal, else dyn. */
function pathType(path: string, template: Record<string, unknown>, records: unknown): ExprType {
  if (!path.startsWith(`/`)) return `dyn`
  const bindings = Array.isArray(template.bindings) ? template.bindings : []
  for (const b of bindings) {
    if (!isObject(b) || typeof b.path !== `string` || typeof b.source !== `string`) continue
    if (path !== b.path && !path.startsWith(`${b.path}/`)) continue
    const parsed = parseSource(b.source)
    const kind = parsed?.scheme === RECORDS_SCHEME ? parsed.name : parsed?.scheme === `exp` && parsed.name === `records` ? parsed.params.kind : undefined
    if (!kind || !isObject(records) || !own(records, kind)) return `dyn`
    const rest = path.slice(b.path.length).split(`/`).filter((s, i) => i > 0 || s !== ``)
    return walk(recordsValueType(records[kind]), rest.map(unescape))
  }
  let cur: unknown = template.data
  for (const seg of path === `/` ? [] : path.slice(1).split(`/`).map(unescape)) {
    if (Array.isArray(cur) && /^\d+$/.test(seg)) cur = cur[Number(seg)]
    else if (isObject(cur) && own(cur, seg)) cur = cur[seg]
    else return `dyn`
  }
  return cur === undefined ? `dyn` : literalType(cur)
}

/** A var's type; one the checker cannot type may be missing (`dyn?`). */
function varType(v: unknown, template: Record<string, unknown>, records: unknown): ExprType {
  if (isBinding(v)) {
    const t = pathType(v.path, template, records)
    return t === `dyn` ? optionalType(t) : t
  }
  if (isCall(v)) return optionalType(`dyn`)
  if (Array.isArray(v)) {
    const items = v.map((x) => varType(x, template, records))
    if (items.length === 0) return [`dyn`]
    return [items.every((t) => JSON.stringify(t) === JSON.stringify(items[0])) ? items[0]! : `dyn`]
  }
  if (isObject(v)) {
    const out: Record<string, ExprType> = {}
    for (const [k, x] of Object.entries(v)) out[k] = varType(x, template, records)
    return out
  }
  return typeName(v)
}

/** Every `expr` call in a package's templates, checked. */
export function validatePackageExpressions(pkg: Record<string, unknown>): ExpressionIssue[] {
  const issues: ExpressionIssue[] = []
  if (!isObject(pkg.templates)) return issues
  for (const [id, template] of Object.entries(pkg.templates)) {
    if (!isObject(template) || !Array.isArray(template.components)) continue
    const visit = (value: unknown, at: string) => {
      if (Array.isArray(value)) return value.forEach((v, i) => visit(v, `${at}/${i}`))
      if (!isObject(value)) return
      if (value.call === `expr` && isObject(value.args)) {
        const args = value.args
        const here = `${at}/args`
        // VAPP-195: every expression binding shows a default when it fails.
        if (!Object.prototype.hasOwnProperty.call(args, `default`)) issues.push({ path: `${here}/default`, message: `an expression binding needs a default, shown when it fails` })
        if (typeof args.expr !== `string`) issues.push({ path: `${here}/expr`, message: `expr is a string` })
        else if (args.vars !== undefined && !isObject(args.vars)) issues.push({ path: `${here}/vars`, message: `vars is an object` })
        else {
          const types: Record<string, ExprType> = {}
          for (const [name, v] of Object.entries((args.vars as Record<string, unknown> | undefined) ?? {})) types[name] = varType(v, template, pkg.records)
          const checked = checkExpression(args.expr, types)
          if (`error` in checked) issues.push({ path: `${here}/expr`, message: checked.error })
        }
      }
      for (const [k, v] of Object.entries(value)) visit(v, `${at}/${escape(k)}`)
    }
    template.components.forEach((c, i) => visit(c, `/templates/${escape(id)}/components/${i}`))
  }
  return issues
}
