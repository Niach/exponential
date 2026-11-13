// #EXP-1725: what the IDE's shell regions show, a pure projection of the
// host's state. `fixtures/ide-sidebar.json` is the spec; the document
// (`apps/ede/ui/sidebar.json`) binds the result under /shell/ide.
import fixture from "../fixtures/ide-sidebar.json" with { type: "json" }

export type IdeDot = `amber` | `green` | `primary`

/** #FEED-232 (`nav-badges.json`): a nav row's count badge tone; no dots. */
export type IdeTone = `attention` | `muted`

/** A count the host computed (`chatsBadge`): 0 = no badge. */
export interface IdeCount {
  count: number
  tone: IdeTone
}

export interface IdeSidebarInput {
  /** The open screen as `<kind>` or `<kind>:<id>`. */
  screen: string
  needsYou: number
  drafts: number
  chats: IdeCount
  owner: boolean
  pins: { id: string; identifier: string; title: string }[]
  agents: { id: string; name: string; icon: string; needsYou: boolean; unread: number }[]
  apps: { id: string; slug: string; name: string; icon: string }[]
  boards: { id: string; name: string; icon: string }[]
  active: { id: string; title: string; icon: string; sessionIds: string[]; unread: boolean }[]
  /** #EXP-1616: the Exponential repositories, each with its live PR tree (`repoPrTree`, placed by the host). */
  repos?: { id: string; name: string; prs: { id: string; identifier: string; title: string; subtitle: string; depth: number; session?: string | null }[] }[]
}

/** #EXP-1616: a Pull requests tree row: a repository at depth 0, its PRs under it. */
export interface IdePrRow {
  id: string
  repo: string
  title: string
  identifier: string | null
  subtitle: string | null
  icon: string | null
  depth: number
  selected: boolean
}

export interface IdeNavRow {
  id: string
  title: string
  icon: string
  selected: boolean
  count: string | null
  tone: IdeTone | null
}

export interface IdeRow {
  id: string
  title: string
  icon: string
  selected: boolean
  count: string | null
  tone: IdeTone | null
  gear: boolean
}

export interface IdeSidebar {
  nav: IdeNavRow[]
  pins: { id: string; identifier: string; title: string; selected: boolean }[]
  agents: { add: boolean; rows: IdeRow[] }
  apps: { rows: IdeRow[] }
  boards: { rows: IdeRow[]; empty: boolean }
  active: { id: string; title: string; icon: string; selected: boolean; unread: boolean }[]
  prs: IdePrRow[]
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

/** The badge's label: null at zero, `99+` past 99 (nav-badges.json `labelCases`). */
export const countLabel = (count: number): string | null => count <= 0 ? null : count > 99 ? `99+` : String(count)

const badge = (count: number, tone: IdeTone) => {
  const label = countLabel(count)
  return { count: label, tone: label === null ? null : tone }
}

const nav = (id: keyof typeof COPY, icon: string, selected: boolean, count: number, tone: IdeTone): IdeNavRow => ({
  id,
  title: COPY[id],
  icon,
  selected,
  ...badge(count, tone),
})

/** An agent row's badge (`rowBadge`): its unread, at least 1 while it needs
 *  you, attention when it does. Busy is no badge. */
export const agentCount = (unread: number, needsYou: boolean): IdeCount =>
  needsYou ? { count: Math.max(1, unread), tone: `attention` } : { count: unread, tone: `muted` }

export function ideSidebar(input: IdeSidebarInput): IdeSidebar {
  const { screen } = input
  const rows: IdeNavRow[] = []
  if (input.needsYou > 0 || screen === `needs-you`) rows.push(nav(`needs-you`, `nav-needs-you`, screen === `needs-you`, input.needsYou, `attention`))
  rows.push(nav(`chats`, `nav-chats`, screen === `chats`, input.chats.count, input.chats.tone))
  rows.push(nav(`devices`, `nav-devices`, screen === `devices`, 0, `muted`))
  if (input.drafts > 0 || screen === `drafts`) rows.push(nav(`drafts`, `nav-drafts`, screen === `drafts`, input.drafts, `muted`))
  return {
    nav: rows,
    pins: input.pins.map((p) => ({ ...p, selected: screen === `issue:${p.id}` })),
    agents: {
      add: input.owner,
      rows: input.agents.map((a) => ({ id: a.id, title: a.name, icon: a.icon, selected: screen === `agent:${a.id}`, ...badge(agentCount(a.unread, a.needsYou).count, agentCount(a.unread, a.needsYou).tone), gear: true })),
    },
    apps: {
      rows: input.apps.map((a) => ({ id: a.id, title: a.name, icon: a.icon, selected: screen === `app:${a.id}` || screen === `app:${a.slug}`, count: null, tone: null, gear: true })),
    },
    boards: {
      rows: input.boards.map((b) => ({ id: b.id, title: b.name, icon: b.icon, selected: screen === `board:${b.id}`, count: null, tone: null, gear: input.owner })),
      empty: input.boards.length === 0,
    },
    active: input.active.map((a) => ({
      id: a.id,
      title: a.title,
      icon: a.icon,
      selected: screen === `chat:${a.id}` || a.sessionIds.some((id) => screen === `session:${id}`),
      unread: a.unread,
    })),
    prs: (input.repos ?? []).filter((r) => r.prs.length > 0).flatMap((r): IdePrRow[] => [
      { id: r.id, repo: r.id, title: r.name, identifier: null, subtitle: null, icon: `ui-repository`, depth: 0, selected: screen === `repo:${r.id}` },
      ...r.prs.map((p) => ({ id: p.id, repo: r.id, title: p.title, identifier: p.identifier, subtitle: p.subtitle, icon: null, depth: p.depth + 1, selected: p.session != null && (screen === `chat:${p.session}` || screen === `session:${p.session}`) })),
    ]),
  }
}

/** The tab bar: every open tab in tab order, the active one by key. */
export function ideTabs(tabs: IdeTabInput[], active: string | null): IdeTabs {
  return {
    tabs: tabs.map((t) => ({ value: t.key, label: t.title, icon: t.icon })),
    activeTab: tabs.some((t) => t.key === active) ? active : null,
  }
}

export type IdeTabPlacement = { placement: "top" | "bottom"; moreSide: "top" | "bottom" }

/** Where the tab bar sits: on top in the standalone IDE, at the bottom inside Exponential Desktop; +N opens away from the edge. */
export function ideTabPlacement(ede: boolean): IdeTabPlacement {
  return ede ? { placement: "bottom", moreSide: "top" } : { placement: "top", moreSide: "bottom" }
}
