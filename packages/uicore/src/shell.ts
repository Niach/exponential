// #EDE-46: the pure shell reducer the IDE and eDE both host (ide = shell +
// uicore, ede = compositor + uicore). Byte-locked by fixtures/shell.json
// (its $comment is the spec); the uicore-model crate replays the same cases
// in Rust. No clock, no I/O: the host feeds it what the compositor and the
// person do and reads the state through the selectors.

import fixture from "../fixtures/shell.json" with { type: "json" }

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
}

export interface ShellState {
  switcher: { current: SwitcherEntry; entries: SwitcherEntry[] }
  windows: ShellWindow[]
  mru: string[]
  workspaces: { active: number; count: number }
  taskbar: { pinned: string[]; hover: string | null; previews: string | null; menu: string | null }
  startMenu: { open: boolean; query: string; category: string }
  chat: { open: boolean }
  altTab: { open: boolean; selected: number }
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
  | { kind: `chat.toggle` }
  | { kind: `altTab.next` }
  | { kind: `altTab.prev` }
  | { kind: `altTab.commit` }
  | { kind: `altTab.cancel` }

/** The fixture's `initial`: the state at first boot (one local desktop, no windows, one workspace). */
export const INITIAL: ShellState = fixture.initial as ShellState

/** A fixture case's `state`: each top-level key given is complete, the others come from `initial`. */
export function hydrate(partial: Partial<ShellState> | undefined): ShellState {
  return clone({ ...INITIAL, ...(partial ?? {}) } as ShellState)
}

export const clone = (state: ShellState): ShellState => structuredClone(state)

const closedStartMenu = (): ShellState[`startMenu`] => ({ open: false, query: ``, category: `all` })
const closedAltTab = (): ShellState[`altTab`] => ({ open: false, selected: 0 })

function focus(state: ShellState, id: string): void {
  if (!state.windows.some((w) => w.id === id)) return
  for (const w of state.windows) {
    w.focused = w.id === id
    if (w.focused) w.minimized = false
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
    case `chat.toggle`:
      s.chat.open = !s.chat.open
      break
    case `altTab.next`:
    case `altTab.prev`: {
      const n = altTabOrder(s).length
      if (n === 0) break
      const forward = e.kind === `altTab.next`
      if (!s.altTab.open) s.altTab = { open: true, selected: n >= 2 ? (forward ? 1 : n - 1) : 0 }
      else s.altTab.selected = (s.altTab.selected + (forward ? 1 : n - 1)) % n
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
}

const onActive = (state: ShellState): ShellWindow[] => state.windows.filter((w) => w.workspace === state.workspaces.active)

/** Mint's grouped window list: pinned apps first in pin order, then the running unpinned apps in the order their first window appeared; the active workspace only. */
export function taskbarButtons(state: ShellState): TaskbarButton[] {
  const active = onActive(state)
  const apps: string[] = [...state.taskbar.pinned]
  for (const w of active) if (!apps.includes(w.app)) apps.push(w.app)
  return apps.map((app) => {
    const ws = active.filter((w) => w.app === app)
    return {
      app,
      pinned: state.taskbar.pinned.includes(app),
      windows: ws.map((w) => w.id),
      focused: ws.some((w) => w.focused),
      minimized: ws.length > 0 && ws.every((w) => w.minimized),
      running: ws.length > 0,
    }
  })
}

export type TaskbarPress = { action: `launch` } | { action: `minimize`; id: string } | { action: `focus`; id: string } | { action: `previews` }

/** Mint's click on a button: launch, minimize the focused single window, focus it otherwise, or open the previews of a group. */
export function taskbarPress(state: ShellState, app: string): TaskbarPress {
  const ws = onActive(state).filter((w) => w.app === app)
  if (ws.length === 0) return { action: `launch` }
  if (ws.length === 1) return ws[0].focused && !ws[0].minimized ? { action: `minimize`, id: ws[0].id } : { action: `focus`, id: ws[0].id }
  return { action: `previews` }
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

/** The switcher's order: the mru (minimized included), the active workspace's windows first, then the others. */
export function altTabOrder(state: ShellState): string[] {
  const workspaceOf = new Map(state.windows.map((w) => [w.id, w.workspace]))
  const here = state.mru.filter((id) => workspaceOf.get(id) === state.workspaces.active)
  const elsewhere = state.mru.filter((id) => workspaceOf.has(id) && workspaceOf.get(id) !== state.workspaces.active)
  return [...here, ...elsewhere]
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

/** #EDE-65: one chat the host knows (a root session of the current team or Local desktop). */
export interface ChatInput {
  id: string
  title: string
  agent?: string | null
  needsYou: boolean
  busy: boolean
  unread: boolean
  /** The last activity, ISO 8601. */
  at: string
}

export type ChatDot = `amber` | `green` | `primary`

export interface ChatRow {
  id: string
  title: string
  icon: string
  dot: ChatDot | null
}

/** The rail keeps this many chats. */
export const CHATS_MAX = 20

const chatIcon = (agent: string | null | undefined): string => {
  const id = (agent ?? ``).trim()
  if (id === `` || id === `claude`) return `agent:claude`
  return id === `codex` ? `agent:codex` : `settings-agents`
}

/** #EDE-65: the rail's chats: the most recent first (ties by id), at most CHATS_MAX, the IDE's dots. */
export function chatRows(chats: ChatInput[]): ChatRow[] {
  return [...chats]
    .sort((a, b) => (a.at === b.at ? (a.id < b.id ? -1 : a.id > b.id ? 1 : 0) : a.at < b.at ? 1 : -1))
    .slice(0, CHATS_MAX)
    .map((c) => ({
      id: c.id,
      title: c.title.trim() || `Untitled`,
      icon: chatIcon(c.agent),
      dot: c.needsYou ? `amber` : c.busy ? `green` : c.unread ? `primary` : null,
    }))
}
