// VAPP-120: typed host ACTIONS, the pure reference every app host mirrors
// (fixtures/app-actions.json is the spec; desktop `domain::app_actions`, iOS
// `AppActions.swift`, Android `AppActions.kt` replay its cases): which
// `exponential.<noun>.<op>` functions exist, what a name resolves to, what a
// package declares, what an app's grants allow, and the consent card's words.

import fixture from "../fixtures/app-actions.json" with { type: "json" }
import contract from "../contract.json" with { type: "json" }

export const APP_ACTION_PREFIX = fixture.prefix
/** Every function name, sorted. */
export const APP_ACTIONS: readonly string[] = fixture.functions
export const APP_ACTIONS_COPY = fixture.copy
const TOOLS = fixture.tools as Record<string, { tool: string; op: string | null }>

export interface ActionTarget {
  /** The MCP tool (`exponential_issues`). */
  tool: string
  /** The noun's op, null for a verb tool. */
  op: string | null
}

/** `exponential.issues.update_status` → the tool and op it runs; null for
 *  anything that is not an offered action. */
export function parseActionName(name: string): ActionTarget | null {
  const target = Object.prototype.hasOwnProperty.call(TOOLS, name) ? TOOLS[name] : undefined
  return target ? { tool: target.tool, op: target.op } : null
}

/** VAPP-107's pattern rule: an exact name or a `prefix*`. */
function matches(pattern: string, name: string): boolean {
  return pattern.endsWith(`*`) ? name.startsWith(pattern.slice(0, -1)) : pattern === name
}

/** The actions a package's `functions` list names (patterns expanded), in
 *  the fixture's order; anything that is not an offered action is dropped. */
export function declaredActions(functions: readonly string[]): string[] {
  return APP_ACTIONS.filter((name) => functions.some((pattern) => matches(pattern, name)))
}

/** Does the app's `granted_tools` list let `name` run without asking?
 *  Exact names only: a grant is a person's choice, never a pattern. */
export function grantAllows(granted: readonly string[], name: string): boolean {
  return granted.includes(name)
}

const DESTRUCTIVE_WORDS: readonly string[] = [`delete`, `merge`, `close`, `end`, `remove`, `trash`]

export interface ActionConsent {
  tool: string
  title: string
  destructive: boolean
  /** The arguments as compact JSON, null when there are none. */
  args: string | null
}

type DisplayRow = { name: string; title: string; subjectKey: string }
const DISPLAY: readonly DisplayRow[] = (contract.expToolDisplay.tools as DisplayRow[]).map((r) => ({ name: r.name, title: r.title, subjectKey: r.subjectKey }))

/** The display row's title for an action (`Set issue status`), the name
 *  itself for anything else: the settings page's row label. */
export function actionTitle(name: string): string {
  const target = parseActionName(name)
  if (!target) return name
  const rowName = `${target.tool.replace(/^exponential_/, ``)}${target.op ? `_${target.op}` : ``}`
  return DISPLAY.find((r) => r.name === rowName)?.title ?? name
}

/** The consent card's words for a typed action call: the display row's
 *  title lower-cased after "Allow this surface to", the subject arg when the
 *  row names one, the destructive flag from the tool name and op, the args
 *  as compact JSON. Null for a name that is not an offered action (the
 *  generic `exponential-ui-consent` copy applies). */
export function actionConsent(name: string, args: Record<string, unknown>): ActionConsent | null {
  const target = parseActionName(name)
  if (!target) return null
  const rowName = `${target.tool.replace(/^exponential_/, ``)}${target.op ? `_${target.op}` : ``}`
  const row = DISPLAY.find((r) => r.name === rowName)
  const base = row ? `${row.title.charAt(0).toLowerCase()}${row.title.slice(1)}` : name
  const subject = row?.subjectKey ? args[row.subjectKey] : undefined
  const title = `Allow this surface to ${base}${typeof subject === `string` && subject ? ` ${subject}` : ``}?`
  const words = `${target.tool} ${target.op ?? ``}`.toLowerCase().split(/[^a-z]+/)
  const json = JSON.stringify(args)
  return { tool: target.tool, title, destructive: words.some((w) => DESTRUCTIVE_WORDS.includes(w)), args: json === `{}` ? null : json }
}
