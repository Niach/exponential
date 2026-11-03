// VAPP-218 (eApps U1, spec docs/spec/apps.md §5.1): a package's SCREENS,
// the navigation manifest. `screens: [{template, title, icon?, params?}]`
// lists the templates a person can open; the HOST draws the navigation
// from it (a sidebar on wide frames, the phone bottom bar on phones) and
// the app draws no chrome of its own. A screen is keyed by its template.
// `app.navigate {screen, params?}` (a press's functionCall or an action's call
// step) opens one; its params become `/params/*` in the template data, and
// a deep link carries the same pair. Pure: fixtures/host-router.json
// (`screens`) locks it on every platform (Rust `eui_core::host::screens`).

import { ICON_NAMES } from "../catalog.generated"
import type { PackageIssue } from "./package"

/** The most screens a package declares (the phone bottom bar's room). */
export const MAX_SCREENS = 5
/** A screen title's most characters (code points). */
export const MAX_SCREEN_TITLE = 32
/** The host function (and action call) that opens a screen. Always allowed:
 *  it leaves the package's own surface and data untouched. */
export const NAVIGATE_FUNCTION = `app.navigate`
/** The template data key a screen's params land under (`/params/*`). */
export const SCREEN_PARAMS_KEY = `params`

export interface PackageScreen {
  /** The template it opens (= the screen's id, in links and `navigate`). */
  template: string
  title: string
  /** An Exponential UI icon name. */
  icon?: string
  /** The params it opens with (string values); a link's or a navigate's
   *  own params override them key by key. */
  params?: Record<string, string>
}

/** A screen as a host opens it: its template and the merged params. */
export interface ResolvedScreen {
  screen: string
  title: string
  icon: string | null
  params: Record<string, string>
}

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)
const own = (o: Record<string, unknown>, k: string) => Object.prototype.hasOwnProperty.call(o, k)
const ICONS: ReadonlySet<string> = new Set(ICON_NAMES)
const SCREEN_KEYS = [`template`, `title`, `icon`, `params`]
const escape = (k: string) => k.replace(/~/g, `~0`).replace(/\//g, `~1`)

/** Every problem with a package's `screens` (unset = no screens: the host
 *  shows the first template, as before). */
export function validatePackageScreens(pkg: Record<string, unknown>): PackageIssue[] {
  if (pkg.screens === undefined) return []
  const list = pkg.screens
  if (!Array.isArray(list) || !list.length) return [{ path: `/screens`, message: `screens is a non-empty list of {template, title, icon?, params?}` }]
  const issues: PackageIssue[] = []
  if (list.length > MAX_SCREENS) issues.push({ path: `/screens`, message: `at most ${MAX_SCREENS} screens, got ${list.length}` })
  const templates = isObject(pkg.templates) ? pkg.templates : {}
  const seen = new Set<string>()
  list.forEach((s, i) => {
    const at = `/screens/${i}`
    if (!isObject(s)) return void issues.push({ path: at, message: `a screen is {template, title, icon?, params?}` })
    for (const k of Object.keys(s).sort()) if (!SCREEN_KEYS.includes(k)) issues.push({ path: `${at}/${escape(k)}`, message: `unknown screen key ${k}` })
    if (typeof s.template !== `string` || !s.template) issues.push({ path: `${at}/template`, message: `template is a required string` })
    else if (!own(templates, s.template)) issues.push({ path: `${at}/template`, message: `template ${s.template} names no package template` })
    else if (seen.has(s.template)) issues.push({ path: `${at}/template`, message: `duplicate screen ${s.template}` })
    else seen.add(s.template)
    if (typeof s.title !== `string` || !s.title.trim()) issues.push({ path: `${at}/title`, message: `title is a required string` })
    else if ([...s.title].length > MAX_SCREEN_TITLE) issues.push({ path: `${at}/title`, message: `title is at most ${MAX_SCREEN_TITLE} characters` })
    if (s.icon !== undefined && (typeof s.icon !== `string` || !ICONS.has(s.icon))) issues.push({ path: `${at}/icon`, message: `icon ${typeof s.icon === `string` ? s.icon : JSON.stringify(s.icon)} is not an Exponential UI icon` })
    if (s.params !== undefined) {
      if (!isObject(s.params)) issues.push({ path: `${at}/params`, message: `params is an object of strings` })
      else for (const k of Object.keys(s.params).sort()) if (typeof s.params[k] !== `string`) issues.push({ path: `${at}/params/${escape(k)}`, message: `params.${k} is a string` })
    }
  })
  issues.push(...navigateIssues(pkg, seen))
  return issues
}

/** A CEL string literal (`'deals'`, `"deals"`) → its text; else null (a
 *  dynamic screen is checked when it runs). */
function celStringLiteral(v: unknown): string | null {
  if (typeof v !== `string`) return null
  const m = /^\s*(?:'([^'\\]*)'|"([^"\\]*)")\s*$/.exec(v)
  return m ? (m[1] ?? m[2] ?? ``) : null
}

/** Every literal `navigate` target names a screen: a press's
 *  `functionCall {call: app.navigate, args: {screen}}` (a JSON string) and an
 *  action's `{call: app.navigate, args: {screen: <CEL string literal>}}`. */
function navigateIssues(pkg: Record<string, unknown>, screens: ReadonlySet<string>): PackageIssue[] {
  const issues: PackageIssue[] = []
  const check = (screen: string | null, at: string) => {
    if (screen !== null && !screens.has(screen)) issues.push({ path: at, message: `navigate: ${screen} is not a screen` })
  }
  if (isObject(pkg.templates))
    for (const id of Object.keys(pkg.templates).sort()) {
      const template = pkg.templates[id]
      if (!isObject(template) || !Array.isArray(template.components)) continue
      template.components.forEach((c, i) => {
        if (!isObject(c) || !isObject(c.on)) return
        for (const event of Object.keys(c.on).sort()) {
          const handler = c.on[event]
          const fc = isObject(handler) && isObject(handler.functionCall) ? handler.functionCall : null
          if (!fc || fc.call !== NAVIGATE_FUNCTION) continue
          const at = `/templates/${escape(id)}/components/${i}/on/${escape(event)}/functionCall/args`
          const args = fc.args
          if (!isObject(args)) issues.push({ path: at, message: `navigate takes {screen, params?}` })
          else if (typeof args.screen === `string`) check(args.screen, `${at}/screen`)
        }
      })
    }
  if (isObject(pkg.actions))
    for (const name of Object.keys(pkg.actions).sort()) {
      const action = pkg.actions[name]
      const steps = isObject(action) && Array.isArray(action.steps) ? action.steps : []
      steps.forEach((step, i) => {
        if (!isObject(step) || step.call !== NAVIGATE_FUNCTION) return
        const at = `/actions/${escape(name)}/steps/${i}/args`
        if (!isObject(step.args)) issues.push({ path: at, message: `navigate takes {screen, params?}` })
        else check(celStringLiteral(step.args.screen), `${at}/screen`)
      })
    }
  return issues
}

/** The screens of a (valid) package, in declared order; [] = none. */
export function packageScreens(pkg: { screens?: unknown }): PackageScreen[] {
  return Array.isArray(pkg.screens) ? (pkg.screens.filter((s) => isObject(s) && typeof s.template === `string`) as PackageScreen[]) : []
}

/** The screen a host opens for a request (a deep link, a navigate, the
 *  default): the requested one when the package has it, else the FIRST;
 *  its params = the screen's own, then the request's string params key by
 *  key (anything else dropped). Null when the package declares no screens. */
export function resolveScreen(pkg: { screens?: unknown }, screen?: string | null, params?: Record<string, unknown> | null): ResolvedScreen | null {
  const screens = packageScreens(pkg)
  const picked = screens.find((s) => s.template === screen) ?? screens[0]
  if (!picked) return null
  const merged: Record<string, string> = {}
  if (isObject(picked.params)) for (const k of Object.keys(picked.params).sort()) if (typeof picked.params[k] === `string`) merged[k] = picked.params[k]
  if (isObject(params)) for (const k of Object.keys(params).sort()) if (typeof params[k] === `string`) merged[k] = params[k] as string
  return { screen: picked.template, title: picked.title, icon: typeof picked.icon === `string` ? picked.icon : null, params: merged }
}

/** A `navigate` call's args → the target, or why it is refused: `screen`
 *  names one of the package's screens, `params` (optional) is an object
 *  of strings. */
export function navigateTarget(pkg: { screens?: unknown }, args: unknown): { screen: string; params: Record<string, string> } | { error: string } {
  if (!isObject(args)) return { error: `navigate takes {screen, params?}` }
  if (typeof args.screen !== `string` || !args.screen) return { error: `navigate: screen is a required string` }
  if (!packageScreens(pkg).some((s) => s.template === args.screen)) return { error: `navigate: ${args.screen} is not a screen` }
  const params: Record<string, string> = {}
  if (args.params !== undefined && args.params !== null) {
    if (!isObject(args.params)) return { error: `navigate: params is an object of strings` }
    for (const k of Object.keys(args.params).sort()) {
      const v = args.params[k]
      if (typeof v !== `string`) return { error: `navigate: params.${k} is a string` }
      params[k] = v
    }
  }
  return { screen: args.screen, params }
}

/** The data a screen's template opens with: `{params}` over the template's
 *  own data (`mergeData`, so `/params/*`). */
export function screenData(screen: ResolvedScreen): { params: Record<string, string> } {
  return { [SCREEN_PARAMS_KEY]: { ...screen.params } } as { params: Record<string, string> }
}

/** A binding source's `${params.<key>}` filled from a screen's params
 *  (percent-encoded like a query value; a missing key = empty). */
export function fillScreenParams(source: string, params: unknown): string {
  if (!isObject(params) || !source.includes(`\${params.`)) return source
  return source.replace(/\$\{params\.([A-Za-z0-9_]+)\}/g, (_, k: string) => {
    const v = params[k]
    return typeof v === `string` ? encodeURIComponent(v) : ``
  })
}
