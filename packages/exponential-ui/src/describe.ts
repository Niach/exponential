// #FEED-218: a surface as TEXT, for an agent that cannot look at a screen
// (MCP `exponential_apps {op: "render"}`): the reduced tree bound to its data
// (`bindTree`), one line per node (component, id, the props a person reads,
// list sizes), a bound list's first items expanded, and every problem met on
// the way (a failed expression, an empty bound list, a template that is
// missing). Pure: no layout, no pixels; the screenshot op paints those.

import { absolutePath, bindTree, readPath, type DataScope, type ResolveOptions } from "./dynamic"
import type { UiNode } from "./types"

export interface DescribeOptions extends Omit<ResolveOptions, `onError` | `scope`> {
  /** The surface's lifted templates (`ReduceResult.templates`). */
  templates?: Record<string, UiNode>
  /** How many items of a bound list are expanded (default 2). */
  itemsShown?: number
  /** The line cap (default 400); past it the outline says what it cut. */
  maxLines?: number
}

export interface SurfaceDescription {
  outline: string
  issues: string[]
}

/** The props a person reads, in this order, quoted when text. */
const SHOWN = [`text`, `title`, `label`, `heading`, `subtitle`, `description`, `value`, `placeholder`, `content`, `src`, `href`, `name`, `tone`, `variant`]
/** Props whose arrays are data (shown as a count). */
const COUNTED = [`rows`, `options`, `items`, `data`, `series`, `suggestions`, `tabs`, `steps`, `columns`]
const FLAGS = [`checked`, `selected`, `disabled`, `required`, `readOnly`, `open`]
const TEXT_MAX = 60

function short(value: unknown): string {
  if (typeof value === `string`) {
    const flat = value.replace(/\s+/g, ` `).trim()
    return JSON.stringify(flat.length > TEXT_MAX ? `${flat.slice(0, TEXT_MAX - 1)}…` : flat)
  }
  if (typeof value === `number` || typeof value === `boolean`) return String(value)
  if (value === null) return `null`
  if (Array.isArray(value)) return `[${value.length}]`
  return `{…}`
}

function propsLine(props: Record<string, unknown>): string {
  const parts: string[] = []
  for (const key of SHOWN) {
    const v = props[key]
    if (v === undefined || v === `` || (typeof v === `object` && v !== null && !Array.isArray(v))) continue
    parts.push(`${key}=${short(v)}`)
  }
  for (const key of COUNTED) if (Array.isArray(props[key])) parts.push(`${key}=${(props[key] as unknown[]).length}`)
  for (const key of FLAGS) if (props[key] === true) parts.push(key)
  return parts.join(` `)
}

/** Marks a call's fallback in the strict pass (never a real value). */
const FELL_BACK = `\u0000fell-back:`

/** `value` with every call's `args.default` replaced by a marker naming the
 *  call (its expr when it has one), at any depth. */
function markDefaults(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(markDefaults)
  if (!value || typeof value !== `object`) return value
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(value)) out[k] = markDefaults(v)
  if (typeof out[`@call`] === `string` && out.args && typeof out.args === `object` && !Array.isArray(out.args) && (out.args as Record<string, unknown>).default !== undefined) {
    const args = out.args as Record<string, unknown>
    out.args = { ...args, default: `${FELL_BACK}${typeof args.expr === `string` ? args.expr : out[`@call`]}` }
  }
  return out
}

/** Every marker a bound value holds, at any depth. */
function fellBack(value: unknown, into: string[]): void {
  if (typeof value === `string`) {
    if (value.startsWith(FELL_BACK)) into.push(value.slice(FELL_BACK.length))
  } else if (Array.isArray(value)) value.forEach((v) => fellBack(v, into))
  else if (value && typeof value === `object`) Object.values(value).forEach((v) => fellBack(v, into))
}

/** `root` bound to `data` as text (see the file comment). */
export function describeSurface(root: UiNode, data: unknown, options: DescribeOptions = {}): SurfaceDescription {
  const issues: string[] = []
  const seen = new Set<string>()
  const issue = (message: string) => {
    if (seen.has(message)) return
    seen.add(message)
    issues.push(message)
  }
  const itemsShown = options.itemsShown ?? 2
  const maxLines = options.maxLines ?? 400
  const lines: string[] = []
  let cut = 0
  const push = (line: string) => {
    if (lines.length < maxLines) lines.push(line)
    else cut++
  }
  // A call's `default` hides its failure (VAPP-195): a strict pass with
  // marked defaults reports every fallback; the outline shows the defaults.
  const bind = (node: UiNode, scope: DataScope): UiNode | null => {
    const strict = bindTree(markDefaults(node) as UiNode, data, { ...options, scope, onError: (failure) => issue(failure.message) })
    const fell: string[] = []
    const own = (n: UiNode | null) => {
      if (!n) return
      fellBack(n.props, fell)
      fellBack(n.style, fell)
      n.children.forEach(own)
      Object.values(n.slots ?? {}).forEach(own)
    }
    own(strict)
    for (const what of fell) issue(`${what}: gave no value (the default is shown)`)
    return bindTree(node, data, { ...options, scope })
  }

  const walk = (node: UiNode, depth: number, scope: DataScope) => {
    const pad = `  `.repeat(depth)
    const props = propsLine(node.props)
    push(`${pad}${node.component}#${node.id}${props ? ` ${props}` : ``}`)
    for (const [slot, child] of Object.entries(node.slots ?? {})) {
      push(`${pad}  [${slot}]`)
      walk(child, depth + 2, scope)
    }
    for (const child of node.children) walk(child, depth + 1, scope)
    if (!node.template) return
    const path = absolutePath(node.template.path, scope)
    const list = readPath(data, node.template.path, scope)
    const template = options.templates?.[node.template.component]
    if (!template) {
      issue(`${node.id}: its item template ${node.template.component} does not exist`)
      return
    }
    if (!Array.isArray(list)) {
      push(`${pad}  (no list at ${path})`)
      issue(`${node.id}: nothing is bound at ${path} (a binding that resolved nothing, or a wrong path)`)
      return
    }
    push(`${pad}  × ${list.length} ${node.template.component}${list.length > itemsShown ? ` (first ${itemsShown})` : ``}`)
    if (list.length === 0) issue(`${node.id}: the list at ${path} is empty`)
    for (let i = 0; i < Math.min(list.length, itemsShown); i++) {
      const item: DataScope = { base: `${path}/${i}`, item: list[i], index: i }
      const bound = bind(template, item)
      if (bound) walk(bound, depth + 1, item)
    }
  }

  const bound = bind(root, {})
  if (bound) walk(bound, 0, {})
  else push(`(the root is hidden)`)
  if (cut) lines.push(`… ${cut} more lines`)
  return { outline: lines.join(`\n`), issues }
}
