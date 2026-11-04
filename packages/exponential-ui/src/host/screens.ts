// VAPP-218 (eApps U1, spec docs/spec/apps.md §5.1): a package's SCREENS,
// the navigation manifest. `screens: [{template, title, icon?, params?, nav?,
// exposed?}]` lists the templates a person can open; the HOST draws the
// navigation from it (a sidebar on wide frames, the phone bottom bar on
// phones) and the app draws no chrome of its own. A screen is keyed by its
// template. #EXP-1674: `nav: false` = a detail screen, left out of the
// navigation; `exposed` = the public contract other apps link to.
// `app.navigate {screen, params?}` (a press's functionCall or an action's call
// step) opens one; its params become `/params/*` in the template data, and
// a deep link carries the same pair. Pure: fixtures/host-router.json
// (`screens`) locks it on every platform (Rust `eui_core::host::screens`).

import { ICON_NAMES } from "../catalog.generated"
import type { PackageIssue } from "./package"

/** The most NAVIGATION screens a package declares (the phone bottom bar's
 *  room); `nav: false` screens do not count. */
export const MAX_SCREENS = 5
/** A screen title's most characters (code points). */
export const MAX_SCREEN_TITLE = 32
/** An exposed screen's description's most characters (code points). */
export const MAX_EXPOSED_DESCRIPTION = 200
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
  /** #EXP-1674: `false` = a detail screen, outside the sidebar and tab bar. */
  nav?: false
  /** #EXP-1674: the public contract: other apps may link here, with these
   *  checked params. Not access control: the target's own policy still decides
   *  what the viewer sees. */
  exposed?: ExposedScreen
}

export interface ExposedScreen {
  description: string
  params?: Record<string, { required?: boolean; description?: string }>
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
const SCREEN_KEYS = [`template`, `title`, `icon`, `params`, `nav`, `exposed`]
const EXPOSED_KEYS = [`description`, `params`]
const EXPOSED_PARAM_KEYS = [`required`, `description`]
/** A param key `${params.<key>}` can fill. */
const PARAM_KEY = /^[A-Za-z0-9_]+$/
const escape = (k: string) => k.replace(/~/g, `~0`).replace(/\//g, `~1`)

/** Every problem with a package's `screens` (unset = no screens: the host
 *  shows the first template, as before). */
export function validatePackageScreens(pkg: Record<string, unknown>): PackageIssue[] {
  if (pkg.screens === undefined) return []
  const list = pkg.screens
  if (!Array.isArray(list) || !list.length) return [{ path: `/screens`, message: `screens is a non-empty list of {template, title, icon?, params?, nav?, exposed?}` }]
  const issues: PackageIssue[] = []
  const inNav = list.filter((s) => !isObject(s) || s.nav !== false).length
  if (inNav > MAX_SCREENS) issues.push({ path: `/screens`, message: `at most ${MAX_SCREENS} screens in the navigation, got ${inNav}` })
  if (!inNav) issues.push({ path: `/screens`, message: `at least one screen is in the navigation (not nav: false)` })
  const templates = isObject(pkg.templates) ? pkg.templates : {}
  const seen = new Set<string>()
  list.forEach((s, i) => {
    const at = `/screens/${i}`
    if (!isObject(s)) return void issues.push({ path: at, message: `a screen is {template, title, icon?, params?, nav?, exposed?}` })
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
    if (s.nav !== undefined && s.nav !== false) issues.push({ path: `${at}/nav`, message: `nav is false or absent` })
    if (s.exposed !== undefined) issues.push(...exposedIssues(s.exposed, `${at}/exposed`))
  })
  issues.push(...navigateIssues(pkg, seen))
  return issues
}

/** #EXP-1674: what is wrong with a screen's `exposed`. */
function exposedIssues(exposed: unknown, at: string): PackageIssue[] {
  if (!isObject(exposed)) return [{ path: at, message: `exposed is {description, params?}` }]
  const issues: PackageIssue[] = []
  for (const k of Object.keys(exposed).sort()) if (!EXPOSED_KEYS.includes(k)) issues.push({ path: `${at}/${escape(k)}`, message: `unknown exposed key ${k}` })
  if (typeof exposed.description !== `string` || !exposed.description.trim()) issues.push({ path: `${at}/description`, message: `description is a required string` })
  else if ([...exposed.description].length > MAX_EXPOSED_DESCRIPTION) issues.push({ path: `${at}/description`, message: `description is at most ${MAX_EXPOSED_DESCRIPTION} characters` })
  if (exposed.params === undefined) return issues
  if (!isObject(exposed.params)) return [...issues, { path: `${at}/params`, message: `params is an object of {required?, description?}` }]
  for (const k of Object.keys(exposed.params).sort()) {
    const param = exposed.params[k]
    const here = `${at}/params/${escape(k)}`
    if (!PARAM_KEY.test(k)) issues.push({ path: here, message: `param ${k} is letters, digits and _` })
    if (!isObject(param)) {
      issues.push({ path: here, message: `a param is {required?, description?}` })
      continue
    }
    for (const key of Object.keys(param).sort()) if (!EXPOSED_PARAM_KEYS.includes(key)) issues.push({ path: `${here}/${escape(key)}`, message: `unknown param key ${key}` })
    if (param.required !== undefined && typeof param.required !== `boolean`) issues.push({ path: `${here}/required`, message: `required is a boolean` })
    if (param.description !== undefined && typeof param.description !== `string`) issues.push({ path: `${here}/description`, message: `description is a string` })
  }
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

/** #EXP-1674: the screens the host's navigation paints (no `nav: false`). */
export function navScreens(pkg: { screens?: unknown }): PackageScreen[] {
  return packageScreens(pkg).filter((s) => s.nav !== false)
}

/** The screen a host opens for a request (a deep link, a navigate, the
 *  default): the requested one when the package has it (a `nav: false` one
 *  too), else the FIRST in the navigation; its params = the screen's own, then the request's string params key by
 *  key (anything else dropped). Null when the package declares no screens. */
export function resolveScreen(pkg: { screens?: unknown }, screen?: string | null, params?: Record<string, unknown> | null): ResolvedScreen | null {
  const screens = packageScreens(pkg)
  const picked = screens.find((s) => s.template === screen) ?? navScreens(pkg)[0]
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

/** #EXP-1674: what a link FROM ANOTHER APP may open in this package, or why it
 *  is refused (the host's toast never says which). The target (`screen`, else
 *  the first in the navigation) must be `exposed`; its params = the screen's
 *  own, then the request's string params the contract declares (anything else
 *  dropped); a `required` one must be present and non-empty. A person's URL or
 *  an agent's chat link never comes here: it opens any declared screen. */
export function crossAppTarget(pkg: { screens?: unknown }, screen?: string | null, params?: Record<string, unknown> | null): { screen: string; params: Record<string, string> } | { error: string } {
  const picked = screen ? packageScreens(pkg).find((s) => s.template === screen) : navScreens(pkg)[0]
  if (!picked) return { error: `no such screen` }
  if (!isObject(picked.exposed)) return { error: `${picked.template} is not exposed` }
  const declared = isObject(picked.exposed.params) ? picked.exposed.params : {}
  const merged: Record<string, string> = {}
  if (isObject(picked.params)) for (const k of Object.keys(picked.params).sort()) if (typeof picked.params[k] === `string`) merged[k] = picked.params[k]
  if (isObject(params)) for (const k of Object.keys(params).sort()) if (own(declared, k) && typeof params[k] === `string`) merged[k] = params[k] as string
  for (const k of Object.keys(declared).sort()) {
    const param = declared[k]
    if (isObject(param) && param.required === true && !merged[k]) return { error: `${picked.template} needs ${k}` }
  }
  return { screen: picked.template, params: merged }
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
