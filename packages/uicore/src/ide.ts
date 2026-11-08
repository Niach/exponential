// #EXP-1725: what the IDE's shell regions show, a pure projection of the
// host's state. `fixtures/ide-sidebar.json` is the spec; the document
// (`apps/ede/ui/sidebar.json`) binds the result under /shell/ide.
import fixture from "../fixtures/ide-sidebar.json" with { type: "json" }

export type IdeDot = `amber` | `green` | `primary`

export interface IdeSidebarInput {
  /** The open screen as `<kind>` or `<kind>:<id>`. */
  screen: string
  needsYou: number
  drafts: number
  threadsDot: IdeDot | null
  owner: boolean
  pins: { id: string; identifier: string; title: string }[]
  agents: { id: string; name: string; icon: string; needsYou: boolean; busy: boolean }[]
  apps: { id: string; slug: string; name: string; icon: string }[]
  boards: { id: string; name: string; icon: string }[]
  active: { id: string; title: string; icon: string; sessionIds: string[]; unread: boolean }[]
}

export interface IdeNavRow {
  id: string
  title: string
  icon: string
  selected: boolean
  count: string | null
  dot: IdeDot | null
}

export interface IdeRow {
  id: string
  title: string
  icon: string
  selected: boolean
  dot: IdeDot | null
  gear: boolean
}

export interface IdeSidebar {
  nav: IdeNavRow[]
  pins: { id: string; identifier: string; title: string; selected: boolean }[]
  agents: { add: boolean; rows: IdeRow[] }
  apps: { rows: IdeRow[] }
  boards: { rows: IdeRow[]; empty: boolean }
  active: { id: string; title: string; icon: string; selected: boolean; unread: boolean }[]
}

export interface IdeTabInput {
  key: string
  title: string
  icon: string
}

export interface IdeTabs {
  tabs: { value: string; label: string; icon: string }[]
  activeTab: string | null
}

const COPY = fixture.copy

const nav = (id: keyof typeof COPY, icon: string, selected: boolean, count: string | null, dot: IdeDot | null): IdeNavRow => ({
  id,
  title: COPY[id],
  icon,
  selected,
  count,
  dot,
})

const agentDot = (needsYou: boolean, busy: boolean): IdeDot | null => needsYou ? `amber` : busy ? `green` : null

export function ideSidebar(input: IdeSidebarInput): IdeSidebar {
  const { screen } = input
  const rows: IdeNavRow[] = []
  if (input.needsYou > 0 || screen === `needs-you`) rows.push(nav(`needs-you`, `nav-needs-you`, screen === `needs-you`, String(input.needsYou), null))
  rows.push(nav(`threads`, `nav-threads`, screen === `threads`, null, input.threadsDot))
  rows.push(nav(`devices`, `nav-devices`, screen === `devices`, null, null))
  if (input.drafts > 0 || screen === `drafts`) rows.push(nav(`drafts`, `nav-drafts`, screen === `drafts`, String(input.drafts), null))
  return {
    nav: rows,
    pins: input.pins.map((p) => ({ ...p, selected: screen === `issue:${p.id}` })),
    agents: {
      add: input.owner,
      rows: input.agents.map((a) => ({ id: a.id, title: a.name, icon: a.icon, selected: screen === `agent:${a.id}`, dot: agentDot(a.needsYou, a.busy), gear: true })),
    },
    apps: {
      rows: input.apps.map((a) => ({ id: a.id, title: a.name, icon: a.icon, selected: screen === `app:${a.id}` || screen === `app:${a.slug}`, dot: null, gear: true })),
    },
    boards: {
      rows: input.boards.map((b) => ({ id: b.id, title: b.name, icon: b.icon, selected: screen === `board:${b.id}`, dot: null, gear: input.owner })),
      empty: input.boards.length === 0,
    },
    active: input.active.map((a) => ({
      id: a.id,
      title: a.title,
      icon: a.icon,
      selected: screen === `thread:${a.id}` || a.sessionIds.some((id) => screen === `session:${id}`),
      unread: a.unread,
    })),
  }
}

/** The bottom tab bar: every open tab in tab order, the active one by key. */
export function ideTabs(tabs: IdeTabInput[], active: string | null): IdeTabs {
  return {
    tabs: tabs.map((t) => ({ value: t.key, label: t.title, icon: t.icon })),
    activeTab: tabs.some((t) => t.key === active) ? active : null,
  }
}
