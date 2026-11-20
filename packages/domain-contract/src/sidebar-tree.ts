// #EXP-2027: the sidebar tree (web md+ and the desktop IDE), the pure
// reference of `fixtures/sidebar-tree.json` (its `$comment` is the spec).
// Desktop (Rust `domain::sidebar_tree`) replays it.
//
// No active team: Home, Needs you, Chats, Drafts, Devices (each device a
// row), then one row per team holding ONE mixed list of its boards, agents,
// apps and the root sessions the member placed, in their own folders
// (`sidebar-folders.ts` runs unchanged over that list, section `team`).

import fixture from "../fixtures/sidebar-tree.json" with { type: "json" }
import {
  sidebarMove,
  type SidebarEntry,
  type SidebarMove,
  type SidebarRow,
  type SidebarWrites,
} from "./sidebar-folders"

export const SIDEBAR_TREE_COPY = fixture.copy
/** List nesting: team › folder › item. */
export const SIDEBAR_TREE_MAX_DEPTH = fixture.maxDepth
/** The Teams label's + menu. */
export const SIDEBAR_TEAMS_MENU = fixture.teamsMenu as readonly SidebarTeamsMenuItem[]

export type SidebarTeamsMenuItem = `new-team` | `join-team`
export type SidebarRootRow = `home` | `needs-you` | `chats` | `drafts` | `devices`
export type SidebarItemKind = `board` | `agent` | `app` | `session`
export type SidebarNewKind = `board` | `agent` | `app` | `session`

/** The fixed rows above the teams; Needs you and Drafts only while they count. */
export function sidebarRootRows(input: { needsYou: number; drafts: number }): SidebarRootRow[] {
  const rows: SidebarRootRow[] = [`home`]
  if (input.needsYou > 0) rows.push(`needs-you`)
  rows.push(`chats`)
  if (input.drafts > 0) rows.push(`drafts`)
  rows.push(`devices`)
  return rows
}

const byName = <T extends { id: string; name: string }>(a: T, b: T) => {
  const an = a.name.toLocaleLowerCase(`en`)
  const bn = b.name.toLocaleLowerCase(`en`)
  if (an !== bn) return an < bn ? -1 : 1
  return a.id < b.id ? -1 : a.id > b.id ? 1 : 0
}

export interface SidebarDeviceInput {
  id: string
  name: string
  /** The viewer's own device (not a teammate's shared server). */
  mine: boolean
  online: boolean
  sharedTeamIds: readonly string[]
}

export interface SidebarDeviceRow {
  id: string
  mine: boolean
  /** Offline: drawn muted. */
  muted: boolean
  /** A teammate's server: the team whose mark it wears; null on the viewer's own. */
  teamId: string | null
}

/** The Devices row's children: the viewer's own by name, then teammates'
 *  servers shared with one of the viewer's teams, by name. */
export function sidebarDeviceRows(input: {
  devices: readonly SidebarDeviceInput[]
  viewerTeamIds: readonly string[]
}): SidebarDeviceRow[] {
  const mine = input.devices.filter((device) => device.mine).sort(byName)
  const shared = input.devices
    .filter((device) => !device.mine)
    .map((device) => ({ device, teamId: input.viewerTeamIds.find((id) => device.sharedTeamIds.includes(id)) ?? null }))
    .filter((entry) => entry.teamId !== null)
    .sort((a, b) => byName(a.device, b.device))
  return [
    ...mine.map((device) => ({ id: device.id, mine: true, muted: !device.online, teamId: null })),
    ...shared.map(({ device, teamId }) => ({ id: device.id, mine: false, muted: !device.online, teamId })),
  ]
}

/** The viewer's member teams by name; the personal team never shows. */
export function sidebarTeams(teams: readonly { id: string; name: string; kind: string }[]): string[] {
  return teams
    .filter((team) => team.kind !== `personal`)
    .sort(byName)
    .map((team) => team.id)
}

export interface SidebarMenuItem {
  id: `new` | `new-folder` | `settings`
  children: SidebarNewKind[]
}

/** A team row's `…` / right-click menu (settings-nav.json `newEntries`:
 *  board, agent and app are owner-only). */
export function sidebarTeamMenu(input: { owner: boolean }): SidebarMenuItem[] {
  const kinds: SidebarNewKind[] = input.owner ? [`board`, `agent`, `app`, `session`] : [`session`]
  return [
    { id: `new`, children: kinds },
    { id: `new-folder`, children: [] },
    { id: `settings`, children: [] },
  ]
}

export interface SidebarTeamItem {
  id: string
  kind: SidebarItemKind
}

export interface SidebarSessionInput {
  id: string
  teamId: string
  parentId: string | null
}

/** A team's targets in DEFAULT order: boards by sortOrder, agents (never an
 *  Assistant) by sortOrder, apps by name, then the team's root sessions the
 *  member placed (an item entry), in entry order. Feeds `sidebarTree`. */
export function sidebarTeamItems(input: {
  teamId: string
  boards: readonly { id: string; sortOrder: number }[]
  agents: readonly { id: string; sortOrder: number; role: string }[]
  apps: readonly { id: string; name: string }[]
  sessions: readonly SidebarSessionInput[]
  entries: readonly SidebarEntry[]
}): SidebarTeamItem[] {
  const bySort = <T extends { id: string; sortOrder: number }>(a: T, b: T) =>
    a.sortOrder !== b.sortOrder ? a.sortOrder - b.sortOrder : a.id < b.id ? -1 : a.id > b.id ? 1 : 0
  const sessions = new Map(input.sessions.map((session) => [session.id, session]))
  const placed = [...input.entries]
    .filter((entry) => entry.kind === `item` && entry.targetId !== null)
    .sort((a, b) => (a.sortOrder !== b.sortOrder ? a.sortOrder - b.sortOrder : a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
    .map((entry) => sessions.get(entry.targetId as string))
    .filter((session): session is SidebarSessionInput => !!session && isTeamRoot(session, input.teamId))
  return [
    ...[...input.boards].sort(bySort).map((board) => ({ id: board.id, kind: `board` as const })),
    ...input.agents
      .filter((agent) => agent.role === `agent`)
      .sort(bySort)
      .map((agent) => ({ id: agent.id, kind: `agent` as const })),
    ...[...input.apps].sort(byName).map((app) => ({ id: app.id, kind: `app` as const })),
    ...[...new Set(placed)].map((session) => ({ id: session.id, kind: `session` as const })),
  ]
}

const isTeamRoot = (session: SidebarSessionInput, teamId: string) => session.teamId === teamId && session.parentId === null

/** One List row of a team: the team row, then its nodes one level deeper. */
export interface SidebarTreeRow extends Omit<SidebarRow, `kind` | `depth`> {
  kind: `team` | SidebarRow[`kind`]
  depth: 0 | 1 | 2
}

/** The team row (count = its top-level nodes) and, while open, its
 *  `sidebarRows` one level deeper. */
export function sidebarTeamRows(input: { teamId: string; open: boolean; rows: readonly SidebarRow[] }): SidebarTreeRow[] {
  const count = input.rows.filter((row) => row.depth === 0).length
  const team: SidebarTreeRow = { kind: `team`, id: input.teamId, depth: 0, group: true, name: null, open: input.open, count }
  if (!input.open) return [team]
  return [team, ...input.rows.map((row) => ({ ...row, depth: (row.depth + 1) as 1 | 2 }))]
}

/** A root session of the team dropped on its List: appended to the items,
 *  then a plain `sidebarMove`. Null = refused (another team, a child). */
export function sidebarDropSession(input: {
  teamId: string
  items: readonly string[]
  entries: readonly SidebarEntry[]
  session: SidebarSessionInput
  move: SidebarMove
  newFolderId: string
}): SidebarWrites | null {
  if (!isTeamRoot(input.session, input.teamId) || input.move.id !== input.session.id) return null
  const items = input.items.includes(input.session.id) ? input.items : [...input.items, input.session.id]
  return sidebarMove({ items, entries: input.entries, move: input.move, newFolderId: input.newFolderId })
}

/** The device-local expanded flags (no stored flag = open). */
export const SIDEBAR_DEVICES_OPEN_KEY = fixture.openKeys.devices
export function sidebarTeamOpenKey(teamId: string): string {
  return fixture.openKeys.team.replace(`{teamId}`, teamId)
}
