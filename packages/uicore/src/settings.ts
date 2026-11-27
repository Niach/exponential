// #EDE-273: System Settings on eDE (Mint's cinnamon-settings), the TypeScript
// reference of fixtures/system-settings.json (its $comment is the spec); the
// uicore-model crate replays the same cases (settings.rs).

import fixture from "../fixtures/system-settings.json" with { type: "json" }

export type SettingsKind = `module` | `later` | `launch`

export interface SettingsModule {
  id: string
  name: string
  category: string
  icon: string
  keywords: string
  kind: SettingsKind
  command?: string
  opens?: string
  /** #EDE-274: the module's stack switcher (its pages, in order). */
  tabs?: { value: string; label: string }[]
}

export interface SettingsTile {
  id: string
  name: string
  icon: string
  kind: SettingsKind
  later: boolean
  installed: boolean
}

export interface SettingsCategory {
  id: string
  title: string
  modules: SettingsTile[]
}

export interface SettingsState {
  module: string
  query: string
  tab: string
}

export type SettingsEvent = { kind: `open`; id: string } | { kind: `back` } | { kind: `query`; text: string } | { kind: `submit` } | { kind: `link`; target: string } | { kind: `tab`; value: string } | { kind: string }

export interface SettingsNavigation {
  state: SettingsState
  launch: string | null
  title: string
}

export const SETTINGS_TITLE: string = fixture.title
export const SETTINGS_CATEGORIES = fixture.categories as { id: string; title: string }[]
export const SETTINGS_MODULES = fixture.modules as SettingsModule[]
export const SETTINGS_ALIASES = fixture.aliases as Record<string, string>

const matches = (m: SettingsModule, q: string): boolean => m.name.toLowerCase().includes(q) || m.keywords.toLowerCase().includes(q)

/** The grid's categories and tiles for `query` (`installed` = the launch modules whose tool the host finds). */
export function settingsGrid(query: string, installed: string[]): SettingsCategory[] {
  const q = query.trim().toLowerCase()
  return SETTINGS_CATEGORIES.map((c) => ({
    id: c.id,
    title: c.title,
    modules: SETTINGS_MODULES.filter((m) => m.category === c.id && (q === `` || matches(m, q))).map((m) => ({ id: m.id, name: m.name, icon: m.icon, kind: m.kind, later: m.kind === `later`, installed: m.kind === `launch` ? installed.includes(m.id) : true })),
  })).filter((c) => c.modules.length > 0)
}

const titleOf = (module: string): string => SETTINGS_MODULES.find((m) => m.id === module)?.name ?? SETTINGS_TITLE

function resolve(id: string): SettingsModule | undefined {
  const m = SETTINGS_MODULES.find((x) => x.id === id)
  return m?.opens ? SETTINGS_MODULES.find((x) => x.id === m.opens) : m
}

const result = (state: SettingsState, launch: string | null = null): SettingsNavigation => ({ state, launch, title: titleOf(state.module) })

function open(state: SettingsState, id: string, installed: string[]): SettingsNavigation {
  const m = resolve(id)
  if (!m) return result(state)
  if (m.kind === `launch` && installed.includes(m.id)) return result(state, m.command ?? null)
  return result({ module: m.id, query: state.query, tab: firstTab(m) })
}

const firstTab = (m: SettingsModule): string => m.tabs?.[0]?.value ?? ``

/** One navigation step of the window. */
export function settingsNavigate(state: SettingsState, event: SettingsEvent, installed: string[]): SettingsNavigation {
  const s = { ...state }
  switch (event.kind) {
    case `open`:
      return open(s, (event as { id: string }).id, installed)
    case `back`:
      return result({ ...s, module: ``, tab: `` })
    case `query`:
      return result(s.module ? s : { ...s, query: (event as { text: string }).text })
    case `submit`: {
      if (s.module || !s.query.trim()) return result(s)
      const first = settingsGrid(s.query, installed)[0]?.modules[0]
      return first ? open(s, first.id, installed) : result(s)
    }
    case `link`: {
      let t = (event as { target: string }).target
      if (t.startsWith(`~/settings`)) t = t.slice(`~/settings`.length)
      t = t.replace(/^\/+|\/+$/g, ``)
      t = SETTINGS_ALIASES[t] ?? t
      const grid = { module: ``, query: ``, tab: `` }
      const m = t ? resolve(t) : undefined
      if (!m) return result(grid)
      if (m.kind === `launch` && installed.includes(m.id)) return result(grid, m.command ?? null)
      return result({ module: m.id, query: ``, tab: firstTab(m) })
    }
    case `tab`: {
      const value = (event as { value: string }).value
      const m = SETTINGS_MODULES.find((x) => x.id === s.module)
      return result(m?.tabs?.some((t) => t.value === value) ? { ...s, tab: value } : s)
    }
    default:
      return result(s)
  }
}
