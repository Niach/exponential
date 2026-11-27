// #EDE-46: the pure shell reducer the IDE and eDE both host (ide = shell +
// uicore, ede = compositor + uicore). Byte-locked by fixtures/shell.json
// (its $comment is the spec); the uicore-model crate replays the same cases
// in Rust. No clock, no I/O: the host feeds it what the compositor and the
// person do and reads the state through the selectors.

import fixture from "../fixtures/shell.json" with { type: "json" }
import { countBadgeLabel, rowBadge, type CountBadgeTone } from "../../domain-contract/src/nav-badges"

export const SHELL_VERSION: number = fixture.version

export type SwitcherKind = `local` | `team`
export interface SwitcherEntry {
  kind: SwitcherKind
  id: string
  name: string
  /** EDE-56: the Exponential account a team belongs to (its id); a local desktop has none. */
  account?: string
  /** EDE-56: the account's email, the section's label. */
  accountName?: string
}

export interface SwitcherSection {
  kind: `local` | `account`
  id: string
  label: string
  entries: (SwitcherEntry & { current: boolean })[]
}

export interface ShellWindow {
  id: string
  app: string
  title: string
  focused: boolean
  workspace: number
  minimized: boolean
  /** #EDE-255: asks for attention until focused (absent when false). */
  urgent?: boolean
}

export interface ShellState {
  switcher: { current: SwitcherEntry; entries: SwitcherEntry[] }
  windows: ShellWindow[]
  mru: string[]
  workspaces: { active: number; count: number }
  taskbar: { pinned: string[]; hover: string | null; previews: string | null; menu: string | null }
  startMenu: { open: boolean; query: string; category: string }
  altTab: { open: boolean; selected: number; shown: boolean; app: string | null }
}

export type ShellEvent =
  | { kind: `switcher.pick`; entryKind: SwitcherKind; id: string }
  | { kind: `switcher.set`; entries: SwitcherEntry[] }
  | { kind: `window.opened`; id: string; app: string; title: string; workspace: number }
  | { kind: `window.closed`; id: string }
  | { kind: `window.focused`; id: string }
  | { kind: `window.titled`; id: string; title: string }
  | { kind: `window.minimized`; id: string }
  | { kind: `window.restored`; id: string }
  | { kind: `window.moved`; id: string; workspace: number }
  | { kind: `window.urgent`; id: string; urgent: boolean }
  | { kind: `workspace.switched`; active: number }
  | { kind: `workspace.count`; count: number }
  | { kind: `taskbar.pin`; app: string }
  | { kind: `taskbar.unpin`; app: string }
  | { kind: `taskbar.hover`; app: string }
  | { kind: `taskbar.unhover` }
  | { kind: `taskbar.previews`; app: string | null }
  | { kind: `taskbar.menu`; app: string | null }
  | { kind: `startMenu.toggle` }
  | { kind: `startMenu.open` }
  | { kind: `startMenu.close` }
  | { kind: `startMenu.query`; query: string }
  | { kind: `startMenu.category`; category: string }
  | { kind: `altTab.next` }
  | { kind: `altTab.prev` }
  | { kind: `altTab.nextApp` }
  | { kind: `altTab.prevApp` }
  | { kind: `altTab.show` }
  | { kind: `altTab.select`; index: number }
  | { kind: `altTab.commit` }
  | { kind: `altTab.cancel` }

/** The fixture's `initial`: the state at first boot (one local desktop, no windows, one workspace). */
export const INITIAL: ShellState = fixture.initial as ShellState

/** A fixture case's `state`: each top-level key given is complete, the others come from `initial`. */
export function hydrate(partial: Partial<ShellState> | undefined): ShellState {
  return clone({ ...INITIAL, ...(partial ?? {}) } as ShellState)
}

export const clone = (state: ShellState): ShellState => structuredClone(state)

const closedStartMenu = (): ShellState[`startMenu`] => ({ open: false, query: ``, category: `` })
const closedAltTab = (): ShellState[`altTab`] => ({ open: false, selected: 0, shown: false, app: null })

function focus(state: ShellState, id: string): void {
  if (!state.windows.some((w) => w.id === id)) return
  for (const w of state.windows) {
    w.focused = w.id === id
    if (w.focused) {
      w.minimized = false
      delete w.urgent
    }
  }
  state.mru = [id, ...state.mru.filter((m) => m !== id)]
}

function clampAltTab(state: ShellState): void {
  if (!state.altTab.open) return
  const n = altTabOrder(state).length
  if (n === 0) state.altTab = closedAltTab()
  else if (state.altTab.selected >= n) state.altTab.selected = n - 1
}

/** `reduce(state, event) -> state`: pure; an unknown id, section or workspace leaves the state as it was. */
export function reduce(given: ShellState, event: ShellEvent): ShellState {
  const s = clone(given)
  const e = event as Record<string, unknown> & { kind: string }
  switch (e.kind) {
    case `switcher.pick`: {
      const hit = s.switcher.entries.find((x) => x.kind === e.entryKind && x.id === e.id)
      if (hit) {
        s.switcher.current = { ...hit }
        s.startMenu = closedStartMenu()
      }
      break
    }
    case `switcher.set`: {
      const entries = (e.entries as SwitcherEntry[]).map((x) => ({ ...x }))
      if (entries.length === 0) break
      s.switcher.entries = entries
      const kept = entries.find((x) => x.kind === s.switcher.current.kind && x.id === s.switcher.current.id)
      s.switcher.current = { ...(kept ?? entries[0]) }
      break
    }
    case `window.opened`: {
      const id = e.id as string
      if (s.windows.some((w) => w.id === id)) break
      s.windows.push({ id, app: e.app as string, title: e.title as string, focused: false, workspace: e.workspace as number, minimized: false })
      s.mru.push(id)
      break
    }
    case `window.closed`: {
      const id = e.id as string
      const gone = s.windows.find((w) => w.id === id)
      if (!gone) break
      s.windows = s.windows.filter((w) => w.id !== id)
      s.mru = s.mru.filter((m) => m !== id)
      if (s.taskbar.previews === gone.app && !s.windows.some((w) => w.app === gone.app)) s.taskbar.previews = null
      clampAltTab(s)
      break
    }
    case `window.focused`:
      focus(s, e.id as string)
      break
    case `window.titled`: {
      const w = s.windows.find((x) => x.id === e.id)
      if (w) w.title = e.title as string
      break
    }
    case `window.minimized`: {
      const w = s.windows.find((x) => x.id === e.id)
      if (w) {
        w.minimized = true
        w.focused = false
      }
      break
    }
    case `window.restored`: {
      const w = s.windows.find((x) => x.id === e.id)
      if (w) w.minimized = false
      break
    }
    case `window.urgent`: {
      const w = s.windows.find((x) => x.id === e.id)
      if (w) {
        if (e.urgent === true && !w.focused) w.urgent = true
        else delete w.urgent
      }
      break
    }
    case `window.moved`: {
      const w = s.windows.find((x) => x.id === e.id)
      const to = e.workspace as number
      if (w && to >= 0 && to < s.workspaces.count) w.workspace = to
      clampAltTab(s)
      break
    }
    case `workspace.switched`: {
      const active = e.active as number
      if (active >= 0 && active < s.workspaces.count) s.workspaces.active = active
      clampAltTab(s)
      break
    }
    case `workspace.count`: {
      const count = Math.max(1, e.count as number)
      s.workspaces.count = count
      if (s.workspaces.active >= count) s.workspaces.active = count - 1
      for (const w of s.windows) if (w.workspace >= count) w.workspace = count - 1
      break
    }
    case `taskbar.pin`:
      if (!s.taskbar.pinned.includes(e.app as string)) s.taskbar.pinned.push(e.app as string)
      break
    case `taskbar.unpin`:
      s.taskbar.pinned = s.taskbar.pinned.filter((a) => a !== e.app)
      break
    case `taskbar.hover`:
      s.taskbar.hover = e.app as string
      break
    case `taskbar.unhover`:
      s.taskbar.hover = null
      break
    case `taskbar.previews`:
      s.taskbar.previews = (e.app as string | null) ?? null
      if (s.taskbar.previews !== null) {
        s.taskbar.menu = null
        s.startMenu = closedStartMenu()
      }
      break
    case `taskbar.menu`:
      s.taskbar.menu = (e.app as string | null) ?? null
      if (s.taskbar.menu !== null) {
        s.taskbar.previews = null
        s.startMenu = closedStartMenu()
      }
      break
    case `startMenu.toggle`:
      if (s.startMenu.open) s.startMenu = closedStartMenu()
      else openStartMenu(s)
      break
    case `startMenu.open`:
      openStartMenu(s)
      break
    case `startMenu.close`:
      s.startMenu = closedStartMenu()
      break
    case `startMenu.query`:
      s.startMenu.query = e.query as string
      break
    case `startMenu.category`:
      s.startMenu.category = e.category as string
      break
    case `altTab.next`:
    case `altTab.prev`:
    case `altTab.nextApp`:
    case `altTab.prevApp`: {
      const forward = e.kind === `altTab.next` || e.kind === `altTab.nextApp`
      if (s.altTab.open) {
        const n = altTabOrder(s).length
        if (n > 0) s.altTab.selected = (s.altTab.selected + (forward ? 1 : n - 1)) % n
        break
      }
      const first = altTabOrder(s)[0]
      if (first === undefined) break
      const app = e.kind === `altTab.nextApp` || e.kind === `altTab.prevApp` ? (s.windows.find((w) => w.id === first)?.app ?? null) : null
      s.altTab = { open: true, selected: 0, shown: false, app }
      const n = altTabOrder(s).length
      s.altTab.selected = n >= 2 ? (forward ? 1 : n - 1) : 0
      break
    }
    case `altTab.show`:
      if (s.altTab.open) s.altTab.shown = true
      break
    case `altTab.select`: {
      const index = e.index as number
      if (s.altTab.open && Number.isInteger(index) && index >= 0 && index < altTabOrder(s).length) s.altTab.selected = index
      break
    }
    case `altTab.commit`: {
      if (!s.altTab.open) break
      const id = altTabOrder(s)[s.altTab.selected]
      if (id !== undefined) focus(s, id)
      s.altTab = closedAltTab()
      break
    }
    case `altTab.cancel`:
      s.altTab = closedAltTab()
      break
  }
  return s
}

function openStartMenu(s: ShellState): void {
  s.startMenu.open = true
  s.taskbar.previews = null
  s.taskbar.menu = null
}

// ---- selectors -------------------------------------------------------------

export interface TaskbarButton {
  app: string
  pinned: boolean
  windows: string[]
  focused: boolean
  minimized: boolean
  running: boolean
  /** #EDE-255: one of its windows asks for attention (absent when false). */
  urgent?: boolean
}

const onActive = (state: ShellState): ShellWindow[] => state.windows.filter((w) => w.workspace === state.workspaces.active)

/** Mint's grouped window list: pinned apps first in pin order, then the running unpinned apps in the order their first window appeared; the active workspace only. */
export function taskbarButtons(state: ShellState): TaskbarButton[] {
  const active = onActive(state)
  const apps: string[] = [...state.taskbar.pinned]
  for (const w of active) if (!apps.includes(w.app)) apps.push(w.app)
  return apps.map((app) => {
    const ws = active.filter((w) => w.app === app)
    const button: TaskbarButton = {
      app,
      pinned: state.taskbar.pinned.includes(app),
      windows: ws.map((w) => w.id),
      focused: ws.some((w) => w.focused),
      minimized: ws.length > 0 && ws.every((w) => w.minimized),
      running: ws.length > 0,
    }
    if (ws.some((w) => w.urgent === true)) button.urgent = true
    return button
  })
}

export type TaskbarPress = { action: `launch` } | { action: `minimize`; id: string } | { action: `focus`; id: string }

/** #EDE-254: the group's last focused window: of the app's active-workspace windows the one earliest in the mru, else the first. */
export function lastFocused(state: ShellState, app: string): ShellWindow | null {
  const ws = onActive(state).filter((w) => w.app === app)
  const byMru = (list: ShellWindow[]): ShellWindow | null => {
    for (const id of state.mru) {
      const hit = list.find((w) => w.id === id)
      if (hit) return hit
    }
    return list[0] ?? null
  }
  // #EDE-255: an urgent one first (Cinnamon makes the window that demands attention the group's lastFocused).
  return byMru(ws.filter((w) => w.urgent === true)) ?? byMru(ws)
}

/** #EDE-255: grouped-window-list's flash (FLASH_INTERVAL 500 ms, FLASH_MAX_COUNT 4): is an urgent group's highlight lit `ms` after the call? */
export const TASKBAR_FLASH_MS = 500
export function urgentLit(ms: number): boolean {
  const phase = Math.floor(ms / TASKBAR_FLASH_MS)
  return phase >= 5 || phase % 2 === 1
}

/** Mint's click on a button (grouped-window-list's left-click-action 2): launch, else toggle the group's last focused window (minimize it while it has the focus, focus it otherwise). */
export function taskbarPress(state: ShellState, app: string): TaskbarPress {
  const w = lastFocused(state, app)
  if (!w) return { action: `launch` }
  return w.focused && !w.minimized ? { action: `minimize`, id: w.id } : { action: `focus`, id: w.id }
}

/** #EDE-254: the middle button (middle-click-action 3): the group's last focused window closes. */
export function taskbarMiddle(state: ShellState, app: string): { action: `close`; id: string } | null {
  const w = lastFocused(state, app)
  return w ? { action: `close`, id: w.id } : null
}

export interface MenuRow {
  kind?: `separator` | `submenu` | `checkbox`
  label?: string
  value?: string
  icon?: string
  checked?: boolean
  disabled?: boolean
  items?: MenuRow[]
}

export interface TaskbarMenuContext {
  outputs?: number
  windows?: Record<string, { output?: number; maximized?: boolean; sticky?: boolean }>
  recent?: { name: string; uri: string }[]
  actions?: { id: string; name: string }[]
  entry?: boolean
}

const RECENT_MAX = 10

/** #EDE-254: a group's right-click menu, grouped-window-list's menus.js in its order (shell.json's $comment). */
export function taskbarMenu(state: ShellState, app: string, context: TaskbarMenuContext = {}): MenuRow[] {
  const outputs = context.outputs ?? 1
  const ws = onActive(state).filter((w) => w.app === app)
  const w = lastFocused(state, app)
  const facts = (w && context.windows?.[w.id]) || {}
  const rows: MenuRow[] = []
  const sep = () => rows.push({ kind: `separator` })
  if (w) {
    if (outputs >= 2) {
      const own = facts.output ?? 0
      for (let i = 0; i < outputs; i++) if (i !== own) rows.push({ label: outputs === 2 ? `Move to the other monitor` : `Move to monitor ${i + 1}`, value: `monitor:${i}` })
    }
    const count = state.workspaces.count
    // #EDE-255: menus.js: a sticky window offers to stay on this workspace only, any other to be on all, then the moves.
    if (count >= 2 && facts.sticky) rows.push({ label: `Only on this workspace`, value: `unstick` })
    else if (count >= 2) {
      rows.push({ label: `Visible on all workspaces`, value: `stick` })
      const items: MenuRow[] = []
      for (let i = 0; i < count; i++) items.push(i === w.workspace ? { label: `Workspace ${i + 1}`, value: `workspace:${i}`, disabled: true } : { label: `Workspace ${i + 1}`, value: `workspace:${i}` })
      rows.push({ kind: `submenu`, label: `Move to another workspace`, items })
    }
    sep()
  }
  rows.push({ kind: `submenu`, label: `Applet preferences`, items: [{ label: `About...`, value: `about`, icon: `ui-help` }, { label: `Configure...`, value: `configure`, icon: `nav-settings` }] })
  sep()
  const recent = context.recent ?? []
  if (recent.length > 0) {
    rows.push({ kind: `submenu`, label: `Recent`, items: recent.slice(0, RECENT_MAX).map((r) => ({ label: r.name, value: `recent:${r.uri}`, icon: `ui-add` })) })
    sep()
  }
  if (context.entry ?? true) {
    sep()
    for (const a of context.actions ?? []) rows.push({ label: a.name, value: `action:${a.id}`, icon: `nav-apps` })
    sep()
    rows.push({ kind: `checkbox`, label: `Pinned to the taskbar`, value: `pin`, icon: `ui-pin`, checked: state.taskbar.pinned.includes(app) })
  } else {
    sep()
  }
  sep()
  if (w) {
    rows.push(w.minimized ? { label: `Restore`, value: `restore`, icon: `ui-redock` } : { label: `Minimize`, value: `minimize`, icon: `ui-minimize` })
    rows.push(facts.maximized ? { label: `Unmaximize`, value: `unmaximize`, icon: `ui-fullscreen-exit` } : { label: `Maximize`, value: `maximize`, icon: `ui-maximize` })
    if (ws.length > 1) rows.push({ label: `Close others`, value: `close-others`, icon: `ui-close` }, { label: `Close all`, value: `close-all`, icon: `nav-sign-out` })
    else rows.push({ label: `Close`, value: `close`, icon: `ui-close` })
    sep()
  }
  // Cinnamon's PopupMenu hides a leading, a trailing and a doubled separator.
  const out: MenuRow[] = []
  for (const r of rows) {
    if (r.kind === `separator` && (out.length === 0 || out[out.length - 1].kind === `separator`)) continue
    out.push(r)
  }
  while (out.length > 0 && out[out.length - 1].kind === `separator`) out.pop()
  return out
}

/** Mint's scroll on a button: cycles the group's windows from the focused one (down = next, up = previous, wrapping); none focused = the first (down) or the last (up). */
export function taskbarScroll(state: ShellState, app: string, direction: `up` | `down`): { action: `focus`; id: string } | null {
  const ws = onActive(state).filter((w) => w.app === app)
  if (ws.length === 0) return null
  const at = ws.findIndex((w) => w.focused)
  const n = ws.length
  const next = direction === `down` ? (at + 1) % n : at < 0 ? n - 1 : (at + n - 1) % n
  return { action: `focus`, id: ws[next].id }
}

export interface WorkspaceBadge {
  index: number
  active: boolean
  count: number
}

/** One badge per workspace with its window count (minimized included). */
export function workspaceBadges(state: ShellState): WorkspaceBadge[] {
  return Array.from({ length: state.workspaces.count }, (_, index) => ({
    index,
    active: index === state.workspaces.active,
    count: state.windows.filter((w) => w.workspace === index).length,
  }))
}

/** EDE-56: the switcher's sections: the Local desktops first, then one per account in first-appearance order. */
export function switcherSections(state: ShellState): SwitcherSection[] {
  const current = state.switcher.current
  const mark = (e: SwitcherEntry) => ({ ...e, current: e.kind === current.kind && e.id === current.id })
  const sections: SwitcherSection[] = [{ kind: `local`, id: `local`, label: `Local desktops`, entries: state.switcher.entries.filter((e) => e.kind === `local`).map(mark) }]
  for (const entry of state.switcher.entries) {
    if (entry.kind === `local`) continue
    const id = entry.account ?? ``
    let section = sections.find((s) => s.kind === `account` && s.id === id)
    if (!section) {
      section = { kind: `account`, id, label: entry.accountName ?? (id || `Exponential`), entries: [] }
      sections.push(section)
    }
    section.entries.push(mark(entry))
  }
  return sections
}

/** The switcher's order: the active workspace's windows in mru order (minimized included); with `altTab.app` only that app's. */
export function altTabOrder(state: ShellState): string[] {
  const byId = new Map(state.windows.map((w) => [w.id, w]))
  return state.mru.filter((id) => {
    const w = byId.get(id)
    return w !== undefined && w.workspace === state.workspaces.active && (state.altTab.app === null || w.app === state.altTab.app)
  })
}

export interface StartMenuApp {
  id: string
  name: string
  comment: string
  categories: string[]
  keywords: string
}

export interface StartMenuEntry {
  id: string
  name: string
  comment: string
  category: string
}

/** EDE-65: one issue-search hit for the start menu (the engine ranks every source as one pool). */
export interface StartMenuIssue {
  id: string
  identifier: string
  title: string
  status: string
  source: SwitcherKind
  sourceId: string
  sourceName: string
  board: string
}

export interface StartMenuIssueRow {
  id: string
  identifier: string
  title: string
  status: string
  icon: string
  source: SwitcherKind
  sourceId: string
  context: string
}

export interface StartMenuCategory {
  id: string
  label: string
  count: number
}

/** Mint's main groups in its order; `match` = the freedesktop categories that put an app there. */
export const START_MENU_GROUPS: ReadonlyArray<{ id: string; label: string; match: ReadonlyArray<string> }> = [
  { id: `all`, label: `All applications`, match: [] },
  { id: `favourites`, label: `Favourites`, match: [] },
  { id: `exponential`, label: `Exponential`, match: [] },
  { id: `accessories`, label: `Accessories`, match: [`Utility`] },
  { id: `administration`, label: `Administration`, match: [`System`, `Settings`] },
  { id: `education`, label: `Education`, match: [`Education`] },
  { id: `games`, label: `Games`, match: [`Game`] },
  { id: `graphics`, label: `Graphics`, match: [`Graphics`] },
  { id: `internet`, label: `Internet`, match: [`Network`] },
  { id: `office`, label: `Office`, match: [`Office`] },
  { id: `programming`, label: `Programming`, match: [`Development`] },
  { id: `science`, label: `Science`, match: [`Science`] },
  { id: `sound-video`, label: `Sound & Video`, match: [`AudioVideo`, `Audio`, `Video`] },
  { id: `other`, label: `Other`, match: [] },
]

export const EXPONENTIAL_APP_PREFIX = `at.exponential.`

/** The groups an app belongs to (not all/favourites), in Mint's order; `other` when none matches. */
export function appGroups(app: StartMenuApp): string[] {
  const out: string[] = []
  if (app.id.startsWith(EXPONENTIAL_APP_PREFIX)) out.push(`exponential`)
  for (const g of START_MENU_GROUPS) if (g.match.some((c) => app.categories.includes(c))) out.push(g.id)
  if (out.length === 0) out.push(`other`)
  return out
}

const byName = (a: StartMenuApp, b: StartMenuApp): number => {
  const x = a.name.toLowerCase()
  const y = b.name.toLowerCase()
  return x < y ? -1 : x > y ? 1 : a.id < b.id ? -1 : a.id > b.id ? 1 : 0
}

const entry = (app: StartMenuApp): StartMenuEntry => ({ id: app.id, name: app.name, comment: app.comment, category: appGroups(app)[0] })

/** The rank of an app for a query that every word of matches somewhere: 0 = the name starts with the query, 1 = every word starts a word of the name, 2 = found in the comment or keywords; -1 = no match. */
export function queryRank(app: StartMenuApp, query: string): number {
  const words = query.split(/\s+/).filter(Boolean)
  const name = app.name.toLowerCase()
  const haystack = `${name} ${app.comment.toLowerCase()} ${app.keywords.toLowerCase()}`
  if (!words.every((w) => haystack.includes(w))) return -1
  if (name.startsWith(query)) return 0
  const nameWords = name.split(/\s+/)
  if (words.every((w) => nameWords.some((n) => n.startsWith(w)))) return 1
  return 2
}

/** The menu's app list: a query (≥ 1 char after trim + lowercase) ranks over every app; else the state's category, All, Favourites (in their order) or one group, by name. */
export function startMenuApps(state: ShellState, apps: StartMenuApp[], favourites: string[]): StartMenuEntry[] {
  const query = state.startMenu.query.trim().toLowerCase()
  if (query.length >= 1) {
    return apps
      .map((app) => ({ app, rank: queryRank(app, query) }))
      .filter((x) => x.rank >= 0)
      .sort((a, b) => a.rank - b.rank || byName(a.app, b.app))
      .map((x) => entry(x.app))
  }
  const category = state.startMenu.category
  if (category === `all`) return [...apps].sort(byName).map(entry)
  if (category === `favourites`) return favourites.flatMap((id) => apps.filter((a) => a.id === id)).map(entry)
  return apps
    .filter((a) => appGroups(a).includes(category))
    .sort(byName)
    .map(entry)
}

/** The category column: Mint's order, the non-empty groups only, All and Favourites always. */
export function startMenuCategories(apps: StartMenuApp[], favourites: string[] = []): StartMenuCategory[] {
  const counts = new Map<string, number>()
  for (const app of apps) for (const g of appGroups(app)) counts.set(g, (counts.get(g) ?? 0) + 1)
  const favouriteCount = favourites.filter((id) => apps.some((a) => a.id === id)).length
  const out: StartMenuCategory[] = []
  for (const g of START_MENU_GROUPS) {
    const count = g.id === `all` ? apps.length : g.id === `favourites` ? favouriteCount : (counts.get(g.id) ?? 0)
    if (g.id === `all` || g.id === `favourites` || count > 0) out.push({ id: g.id, label: g.label, count })
  }
  return out
}

/** EDE-65: the start menu's issue rows under its apps. */
export const START_MENU_ISSUE_LIMIT = 8

const ISSUE_ANCHORS = [`backlog`, `in_progress`, `in_review`, `done`, `cancelled`, `duplicate`]

/** The ranked hits as rows: none without a query, else the first eight in the engine's order with a status icon and `<source> · <board>`. */
export function startMenuIssues(state: ShellState, issues: StartMenuIssue[]): StartMenuIssueRow[] {
  if (state.startMenu.query.trim().length === 0) return []
  return issues.slice(0, START_MENU_ISSUE_LIMIT).map((i) => ({
    id: i.id,
    identifier: i.identifier,
    title: i.title,
    status: i.status,
    icon: ISSUE_ANCHORS.includes(i.status) ? `status-${i.status.replaceAll(`_`, `-`)}` : `ui-issue`,
    source: i.source,
    sourceId: i.sourceId,
    context: i.board ? `${i.sourceName} · ${i.board}` : i.sourceName,
  }))
}

/** #EDE-65: one chat the host knows (a root session of the current team or Local desktop). */
export interface RailChatInput {
  id: string
  title: string
  agent?: string | null
  needsYou: boolean
  busy: boolean
  /** The root's unread agent messages (chats.json `unreadCount`). */
  unreadCount: number
  /** The last activity, ISO 8601. */
  at: string
}


export interface RailChatRow {
  id: string
  title: string
  icon: string
  /** THE count badge (nav-badges.json `rowBadge`): its label, null = none. */
  count: string | null
  tone: CountBadgeTone | null
  /** The working mark (live state is no badge). */
  working: boolean
}

/** The rail keeps this many chats. */
export const CHATS_MAX = 20

const chatIcon = (agent: string | null | undefined): string => {
  const id = (agent ?? ``).trim()
  if (id === `` || id === `claude`) return `agent:claude`
  return id === `codex` ? `agent:codex` : `settings-agents`
}

/** #EDE-65: the rail's chats: the most recent first (ties by id), at most CHATS_MAX, each its rowBadge. */
export function chatRows(chats: RailChatInput[]): RailChatRow[] {
  return [...chats]
    .sort((a, b) => (a.at === b.at ? (a.id < b.id ? -1 : a.id > b.id ? 1 : 0) : a.at < b.at ? 1 : -1))
    .slice(0, CHATS_MAX)
    .map((c) => {
      const badge = rowBadge(c)
      const count = countBadgeLabel(badge.count) || null
      return { id: c.id, title: c.title.trim() || `Untitled`, icon: chatIcon(c.agent), count, tone: count === null ? null : badge.tone, working: c.busy }
    })
}

/** #EDE-258: one root of the IDE sidebar tree as the holder publishes it (the feeds' `tree`). */
export interface StartMenuNode {
  id: string
  kind: string
  title: string
  icon: string
  image?: string | null
  count?: number
  tone?: string | null
  key: string | null
  items: StartMenuItem[]
}

/** #EDE-258: one row inside a tree node. */
export interface StartMenuItem {
  id: string
  kind: string
  title: string
  subtitle?: string | null
  icon: string
  image?: string | null
  depth?: number
  count?: number
  tone?: string | null
  key: string | null
}

/** #EDE-258: domain-contract `sidebarLocalRows` with each row's name. */
export interface StartMenuLocalRow {
  kind: `local` | `desktop` | `board`
  id: string
  desktopId: string | null
  depth: number
  title: string
}

/** #EDE-258: one settings module the search can open. */
export interface StartMenuSetting {
  id: string
  title: string
  icon: string
  keywords: string
}

export interface StartMenuNavCategory {
  id: string
  kind: string
  title: string
  icon: string
  image: string | null
  count: number
  tone: string | null
  separator: boolean
  selected: boolean
}

export interface StartMenuRow {
  id: string
  kind: string
  title: string
  subtitle: string | null
  icon: string
  image: string | null
  depth: number
  count: number
  tone: string | null
  identifier: string | null
  heading: string | null
  target: Record<string, unknown> | null
}

export interface StartMenuNav {
  category: string
  categories: StartMenuNavCategory[]
  assistant: boolean
  rows: StartMenuRow[]
}

/** The app groups' icons in the category column (concepts). */
export const START_MENU_GROUP_ICONS: Readonly<Record<string, string>> = {
  all: `layout-grid`,
  favourites: `star`,
  exponential: `brand-exponential`,
  accessories: `wrench`,
  administration: `settings`,
  education: `graduation-cap`,
  games: `gamepad-2`,
  graphics: `palette`,
  internet: `globe`,
  office: `briefcase`,
  programming: `code`,
  science: `flask-conical`,
  "sound-video": `music`,
  other: `nav-apps`,
}

export const START_MENU_PLACES_LIMIT = 8
export const START_MENU_SETTINGS_LIMIT = 4
export const START_MENU_APPS_LIMIT = 8

const row = (r: Partial<StartMenuRow> & Pick<StartMenuRow, `id` | `kind` | `title` | `icon`>): StartMenuRow => ({
  subtitle: null,
  image: null,
  depth: 0,
  count: 0,
  tone: null,
  identifier: null,
  heading: null,
  target: null,
  ...r,
})

/** 0 = the title starts with the query, 1 = every word starts a word of the title, 2 = every word is in the title (or `extra`); -1 = out. */
function titleRank(title: string, query: string, extra = ``): number {
  const words = query.split(/\s+/).filter(Boolean)
  const lower = title.toLowerCase()
  if (lower.startsWith(query)) return 0
  const titleWords = lower.split(/\s+/)
  if (words.every((w) => titleWords.some((t) => t.startsWith(w)))) return 1
  const haystack = `${lower} ${extra.toLowerCase()}`
  return words.every((w) => haystack.includes(w)) ? 2 : -1
}

/** Stable: the best rank first, ties keep the given order. */
function ranked<T>(list: T[], rank: (x: T) => number): T[] {
  return list
    .map((x, i) => ({ x, i, r: rank(x) }))
    .filter((e) => e.r >= 0)
    .sort((a, b) => a.r - b.r || a.i - b.i)
    .map((e) => e.x)
}

const itemRow = (item: StartMenuItem): StartMenuRow =>
  row({
    id: item.id,
    kind: item.kind,
    title: item.title,
    subtitle: item.subtitle ?? null,
    icon: item.icon,
    image: item.image ?? null,
    depth: item.depth ?? 0,
    count: item.count ?? 0,
    tone: item.tone ?? null,
    target: item.key ? { page: item.key } : null,
  })

const appRow = (app: StartMenuEntry): StartMenuRow =>
  row({ id: `app:${app.id}`, kind: `app`, title: app.name, subtitle: app.comment, icon: `nav-apps`, target: { app: app.id } })

const localRow = (r: StartMenuLocalRow, depth: number): StartMenuRow =>
  row({
    id: r.kind === `desktop` ? `local:${r.id}` : `local:${r.desktopId}:${r.id}`,
    kind: r.kind,
    title: r.title,
    icon: r.kind === `desktop` ? `computer` : `square-kanban`,
    depth,
    target: { local: r.desktopId, board: r.kind === `board` ? r.id : null },
  })

const headed = (rows: StartMenuRow[], heading: string): StartMenuRow[] =>
  rows.map((r, i) => (i === 0 ? { ...r, heading } : r))

/** #EDE-258: the start menu as navigation: the sidebar tree's roots, Local, the app groups; a query = Places, Settings, Apps, Issues. */
export function startMenuNav(
  state: ShellState,
  tree: StartMenuNode[],
  local: StartMenuLocalRow[],
  apps: StartMenuApp[],
  favourites: string[],
  settings: StartMenuSetting[],
  issues: StartMenuIssue[]
): StartMenuNav {
  const categories: StartMenuNavCategory[] = tree.map((node) => ({
    id: node.id,
    kind: node.kind,
    title: node.title,
    icon: node.icon,
    image: node.image ?? null,
    count: node.count ?? 0,
    tone: node.tone ?? null,
    separator: false,
    selected: false,
  }))
  const localTitle = `Local`
  if (local.length > 0) {
    categories.push({ id: `local`, kind: `local`, title: localTitle, icon: `computer`, image: null, count: 0, tone: null, separator: false, selected: false })
  }
  const firstApp = categories.length
  for (const group of startMenuCategories(apps, favourites)) {
    categories.push({ id: group.id, kind: `apps`, title: group.label, icon: START_MENU_GROUP_ICONS[group.id] ?? `nav-apps`, image: null, count: 0, tone: null, separator: false, selected: false })
  }
  const firstPlace = categories.findIndex((c) => c.kind === `team` || c.kind === `local`)
  if (firstPlace >= 0) categories[firstPlace].separator = true
  if (firstApp < categories.length) categories[firstApp].separator = true

  const wanted = state.startMenu.category
  const category = categories.some((c) => c.id === wanted)
    ? wanted
    : (categories.find((c) => c.kind === `team`)?.id ?? (local.length > 0 ? `local` : (tree[0]?.id ?? `all`)))
  for (const c of categories) c.selected = c.id === category
  const current = categories.find((c) => c.id === category)

  const query = state.startMenu.query.trim().toLowerCase()
  if (query.length === 0) {
    let rows: StartMenuRow[] = []
    if (current?.kind === `local`) rows = local.filter((r) => r.kind !== `local`).map((r) => localRow(r, Math.max(0, r.depth - 1)))
    else if (current?.kind === `apps`) rows = startMenuApps({ ...state, startMenu: { ...state.startMenu, category } }, apps, favourites).map(appRow)
    else if (current && current.kind !== `assistant`) rows = (tree.find((n) => n.id === category)?.items ?? []).map(itemRow)
    return { category, categories, assistant: current?.kind === `assistant`, rows }
  }

  const places: StartMenuRow[] = []
  const seen = new Set<string>()
  const add = (r: StartMenuRow) => {
    if (seen.has(r.id)) return
    seen.add(r.id)
    places.push({ ...r, depth: 0 })
  }
  for (const node of tree) {
    if (node.key) {
      add(row({ id: node.id, kind: node.kind, title: node.title, icon: node.icon, image: node.image ?? null, count: node.count ?? 0, tone: node.tone ?? null, target: { page: node.key } }))
    }
    for (const item of node.items) if (item.key) add({ ...itemRow(item), subtitle: node.title })
  }
  for (const r of local) if (r.kind !== `local`) add({ ...localRow(r, 0), subtitle: localTitle })

  const placeRows = ranked(places, (r) => titleRank(r.title, query)).slice(0, START_MENU_PLACES_LIMIT)
  const settingRows = ranked(settings, (s) => titleRank(s.title, query, s.keywords))
    .slice(0, START_MENU_SETTINGS_LIMIT)
    .map((s) => row({ id: `settings:${s.id}`, kind: `settings`, title: s.title, icon: s.icon, target: { settings: s.id } }))
  const appRows = startMenuApps(state, apps, favourites).slice(0, START_MENU_APPS_LIMIT).map(appRow)
  const issueRows = startMenuIssues(state, issues).map((i) =>
    row({
      id: `issue:${i.id}`,
      kind: `issue`,
      title: i.title,
      subtitle: i.context,
      icon: i.icon,
      identifier: i.identifier,
      target: { issue: i.identifier, source: i.source, sourceId: i.sourceId },
    })
  )
  return {
    category,
    categories,
    assistant: false,
    rows: [...headed(placeRows, `Places`), ...headed(settingRows, `Settings`), ...headed(appRows, `Apps`), ...headed(issueRows, `Issues`)],
  }
}

/** #EDE-239: one session of the taskbar's session area as the holder publishes it (the feeds' `sessions`: the IDE sidebar's Active band, in its order). */
export interface SessionAreaInput {
  id: string
  title: string
  /** The sidebar's run mark's agent (`claude`, `codex`), null = none known. */
  agent: string | null
  state: `running` | `needs` | `idle`
  team: string | null
  device: string | null
  /** The chat's page (`chat:<root>`). */
  key: string
  /** The working sessions Stop stops. */
  stop: string[]
  /** The waiting card answered in place: a plan's decisions or a question's options (none = Open only). */
  ask: { messageId: string; kind: `plan` | `question`; title: string; options: ({ decision: `approved` | `rejected` } | { answer: string })[] } | null
  trace: { title: string; icon: string; status: `running` | `done` | `error` }[]
}

export interface SessionAreaChip {
  id: string
  title: string
  icon: string
  running: boolean
  needs: boolean
}

export interface SessionAreaAction {
  id: string
  label: string
  primary: boolean
  messageId: string
  decision: `approved` | `rejected` | null
  answer: string | null
}

export interface SessionAreaRow {
  id: string
  title: string
  subtitle: string
  icon: string
  running: boolean
  needs: boolean
  badge: string | null
  trace: SessionAreaInput[`trace`]
  question: string | null
  actions: SessionAreaAction[]
  stop: boolean
  key: string
}

export interface SessionArea {
  count: number
  needs: number
  chips: SessionAreaChip[]
  more: number
  rows: SessionAreaRow[]
}

const AREA = fixture.sessionArea

const sessionIcon = (agent: string | null): string => (agent === `codex` ? `brand-codex` : agent === `claude` ? `brand-claude` : `action-chat`)

/** #EDE-239: the taskbar's session area over the holder's `sessions`: the first `chipsMax` (or the host's `room`, at least 1) as chips (the band's order), the rest a count; every session a row of the panel. */
export function sessionArea(sessions: SessionAreaInput[], room: number = AREA.chipsMax): SessionArea {
  const rows = sessions.map((s): SessionAreaRow => {
    const title = s.title.trim() || AREA.copy.untitled
    const agent = s.agent ? (AREA.copy.agents as Record<string, string>)[s.agent] ?? null : null
    const subtitle = [agent, s.team, s.device].map((p) => (p ?? ``).trim()).filter((p) => p !== ``).join(` · `)
    const needs = s.state === `needs`
    const running = s.state === `running`
    const ask = needs ? s.ask : null
    const options = ask ? ask.options : []
    const plan = options.filter((o): o is { decision: `approved` | `rejected` } => `decision` in o)
    const ordered = plan.length > 0
      ? [...plan.filter((o) => o.decision === `rejected`), ...plan.filter((o) => o.decision === `approved`)]
      : options.slice(0, AREA.optionsMax)
    const actions = ordered.map((o, i): SessionAreaAction => {
      const decision = `decision` in o ? o.decision : null
      const answer = `answer` in o ? o.answer : null
      return {
        id: `${s.id}:${i}`,
        label: decision === `approved` ? AREA.copy.approve : decision === `rejected` ? AREA.copy.decline : (answer ?? ``),
        primary: decision ? decision === `approved` : i === 0,
        messageId: ask?.messageId ?? ``,
        decision,
        answer,
      }
    })
    return {
      id: s.id,
      title,
      subtitle,
      icon: sessionIcon(s.agent),
      running,
      needs,
      badge: needs ? AREA.copy.needsYou : running ? AREA.copy.running : null,
      trace: s.trace.slice(-AREA.traceMax),
      question: ask ? ask.title.trim() || null : null,
      actions,
      stop: s.stop.length > 0,
      key: s.key,
    }
  })
  const chips = rows.slice(0, Math.max(1, Math.min(room, AREA.chipsMax))).map((r) => ({ id: r.id, title: r.title, icon: r.icon, running: r.running, needs: r.needs }))
  return { count: rows.length, needs: rows.filter((r) => r.needs).length, chips, more: rows.length - chips.length, rows }
}
