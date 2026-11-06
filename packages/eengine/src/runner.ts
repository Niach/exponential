// E6 (#VAPP-266 … #VAPP-269): the runner, the prebuilt shell an exported app
// runs in on macOS, Windows, Linux, Android, iOS and the web. It has no chrome
// of its own: its navigation, its Permissions entry, a package that cannot
// load and the failed-action toast are genUI surfaces made of existing
// components, built here once (the Rust twin `eengine::runner` replays the
// same cases; Swift and Kotlin reach it over `eengine-ffi`). Locked by
// `fixtures/runner.json`.

import { navScreens, packageScreens } from "@exponential_app/eui/host"

import fixture from "../fixtures/runner.json" with { type: "json" }
import type { ManifestEntry } from "./capabilities"
import { scopeDef } from "./capabilities"
import type { SurfaceComponent } from "./consent"
import type { NetworkPolicy } from "./providers"

export const RUNNER_COPY = fixture.copy
export const RUNNER_BUNDLE = fixture.bundle
export const RUNNER_SHELL = fixture.shell

export type FormFactor = `wide` | `compact`
export type RunnerFailure = `missing` | `config` | `signature` | `invalid` | `busy`

// ---------------------------------------------------------------- config

/** A bundle's `runner.json`, validated. */
export interface RunnerConfig {
  id: string
  name: string
  version: string
  network: NetworkPolicy
}

export interface RunnerConfigIssue {
  key: string
  code: string
}

const CONFIG_KEYS = Object.keys(fixture.config.keys)

/** `runner.json` → the config (absent optional keys = their default) and its issues, in key order then the extra keys sorted. */
export function runnerConfig(input: unknown): { config: RunnerConfig | null; issues: RunnerConfigIssue[] } {
  if (!input || typeof input !== `object` || Array.isArray(input)) return { config: null, issues: [{ key: ``, code: `notAnObject` }] }
  const raw = input as Record<string, unknown>
  const issues: RunnerConfigIssue[] = []
  const out: Record<string, unknown> = {}
  for (const key of CONFIG_KEYS) {
    const def = (fixture.config.keys as Record<string, { required?: boolean; pattern?: string; maxLength?: number; values?: string[]; default?: string }>)[key]!
    const value = raw[key]
    if (value === undefined) {
      if (def.required) issues.push({ key, code: `missing` })
      else out[key] = def.default
      continue
    }
    const ok =
      typeof value === `string` &&
      (!def.pattern || new RegExp(def.pattern).test(value)) &&
      (!def.maxLength || ([...value].length <= def.maxLength && value.trim().length > 0)) &&
      (!def.values || def.values.includes(value))
    if (ok) out[key] = value
    else issues.push({ key, code: `invalid` })
  }
  const extra = Object.keys(raw)
    .filter((k) => !CONFIG_KEYS.includes(k))
    .sort()
  for (const key of extra) issues.push({ key, code: fixture.config.forbidden.includes(key) ? `forbidden` : `unknown` })
  return { config: issues.length ? null : (out as unknown as RunnerConfig), issues }
}

// ---------------------------------------------------------------- the shell

type Pkg = Record<string, unknown> & { name?: unknown; templates?: unknown; screens?: unknown; capabilities?: unknown }
type Template = Record<string, unknown> & { components?: unknown; data?: unknown }

const manifestOf = (pkg: Pkg): ManifestEntry[] => (Array.isArray(pkg.capabilities) ? (pkg.capabilities as ManifestEntry[]) : [])

/** Does the package declare an `ask` scope (the shell then offers Permissions)? */
export function runnerHasPermissions(pkg: Pkg): boolean {
  return manifestOf(pkg).some((m) => {
    const def = scopeDef(m.id)
    return !!def && !def.scope.hostOnly && def.scope.consent === `ask`
  })
}

const event = (name: string, context?: Record<string, unknown>) => ({ event: context ? { name, context } : { name } })

/** The shell around one screen's template: its components (the template's root becomes `runner-page`) and its extra data. */
function shellComponents(pkg: Pkg, screen: string, components: SurfaceComponent[], formFactor: FormFactor): { components: SurfaceComponent[]; data: Record<string, unknown> | null } {
  const nav = navScreens(pkg)
  const permissions = runnerHasPermissions(pkg)
  const tabs = nav.length >= 2
  if (!tabs && !permissions) return { components, data: null }
  const page = components.map((c) => (c.id === `root` ? { ...c, id: `runner-page` } : c))
  const S = fixture.shell
  if (formFactor === `wide`) {
    const nav_children = [`runner-title`, ...(tabs ? [`runner-screens`] : []), ...(permissions ? [`runner-permissions`] : [])]
    const shell: SurfaceComponent[] = [
      { id: `root`, component: `Stack`, direction: `horizontal`, gap: `none`, align: `stretch`, style: { height: `100%` }, children: [`runner-sidebar`] },
      { id: `runner-sidebar`, component: `Sidebar`, width: S.sidebarWidth, collapseBelow: `never`, children: [`runner-scroll`], slots: { sidebar: `runner-nav` } },
      { id: `runner-scroll`, component: `Box`, style: { display: `flex`, flexDirection: `column`, flexGrow: 1, minHeight: 0, overflowY: `auto` }, children: [`runner-page`] },
      { id: `runner-nav`, component: `Stack`, gap: `sm`, style: { padding: `$spacing.sm` }, children: nav_children },
      { id: `runner-title`, component: `Heading`, text: typeof pkg.name === `string` ? pkg.name : ``, level: S.titleLevel },
    ]
    if (tabs)
      shell.push(
        { id: `runner-screens`, component: `List`, children: nav.map((_, i) => `runner-screen-${i}`) },
        ...nav.map((s, i) => ({
          id: `runner-screen-${i}`,
          component: `Row`,
          title: s.title ?? s.template,
          ...(s.icon ? { icon: s.icon } : {}),
          selected: s.template === screen,
          density: `compact`,
          pressable: true,
          on: { press: event(`runner.navigate`, { screen: s.template }) },
        })),
      )
    if (permissions)
      shell.push({ id: `runner-permissions`, component: `Row`, title: fixture.copy.permissions, icon: S.permissionsIcon, density: `compact`, pressable: true, on: { press: event(`runner.permissions`) } })
    return { components: [...shell, ...page], data: null }
  }
  const bar = [...(tabs ? [`runner-tabs-box`] : []), ...(permissions ? [`runner-permissions`] : [])]
  const shell: SurfaceComponent[] = [
    { id: `root`, component: `Stack`, gap: `none`, style: { height: `100%` }, children: [`runner-main`, `runner-separator`, `runner-bar`] },
    { id: `runner-main`, component: `Box`, style: { display: `flex`, flexDirection: `column`, flexGrow: 1, minHeight: 0, overflowY: `auto` }, children: [`runner-page`] },
    { id: `runner-separator`, component: `Separator` },
    { id: `runner-bar`, component: `Stack`, direction: `horizontal`, gap: `sm`, align: `center`, justify: `end`, style: { padding: `$spacing.sm` }, children: bar },
  ]
  if (tabs)
    shell.push(
      { id: `runner-tabs-box`, component: `Box`, style: { display: `flex`, flexDirection: `column`, flexGrow: 1, minWidth: 0 }, children: [`runner-tabs`] },
      {
        id: `runner-tabs`,
        component: `Tabs`,
        tabs: nav.map((s) => ({ label: s.title ?? s.template, value: s.template, ...(s.icon ? { icon: s.icon } : {}) })),
        value: { path: `/runner/screen` },
        fill: true,
        on: { change: event(`runner.navigate`, { screen: { path: `/runner/screen` } }) },
      },
    )
  if (permissions)
    shell.push({ id: `runner-permissions`, component: `Button`, icon: S.permissionsIcon, size: `icon`, variant: `ghost`, on: { press: event(`runner.permissions`) } })
  return { components: [...shell, ...page], data: tabs ? { runner: { screen } } : null }
}

/**
 * The package the runner's painter installs: every screen's template wrapped
 * in the shell for `formFactor`. A package without screens gets one, its first
 * template. The Engine keeps the ORIGINAL package (its signature, its grants);
 * only the painter sees this one.
 */
export function runnerPackage(pkg: Pkg, formFactor: FormFactor): Pkg {
  const templates = pkg.templates && typeof pkg.templates === `object` ? (pkg.templates as Record<string, Template>) : {}
  let out: Pkg = pkg
  if (packageScreens(pkg).length === 0) {
    const first = Object.keys(templates)[0]
    if (!first) return pkg
    out = { ...pkg, screens: [{ template: first, title: typeof pkg.name === `string` ? pkg.name : first }] }
  }
  const wrapped: Record<string, Template> = { ...templates }
  for (const s of packageScreens(out)) {
    const t = templates[s.template]
    if (!t || !Array.isArray(t.components)) continue
    const { components, data } = shellComponents(out, s.template, t.components as SurfaceComponent[], formFactor)
    if (components === t.components) continue
    const base = t.data && typeof t.data === `object` && !Array.isArray(t.data) ? (t.data as Record<string, unknown>) : {}
    wrapped[s.template] = { ...t, components, ...(data ? { data: { ...base, ...data } } : {}) }
  }
  return { ...out, templates: wrapped }
}

// ---------------------------------------------------------------- surfaces

/** What the runner paints instead of the app when its bundle cannot load. */
export function runnerFailure(kind: RunnerFailure): SurfaceComponent[] {
  return [{ id: `root`, component: `EmptyState`, icon: fixture.shell.failureIcon, title: fixture.copy.failed.title, description: fixture.copy.failed[kind] }]
}

/** The failed-action toast (a refused call, a refused link): surface `toast`. */
export function runnerToast(message: string): SurfaceComponent[] {
  return [{ id: `root`, component: `Toast`, title: message, type: `error` }]
}
