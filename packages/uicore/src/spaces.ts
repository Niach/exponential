// #EXP-2024: the live view of an agent's space on Exponential Desktop, the
// TypeScript reference of fixtures/spaces.json (its $comment is the spec);
// the uicore-model crate replays the same cases (spaces.rs).

import fixture from "../fixtures/spaces.json" with { type: "json" }
import { LAYOUT } from "./layout"

/** A space the compositor lists: its id (`space-<n>`) and its grant's session. */
export interface SpaceInput {
  id: string
  session: string
}

/** A session row one of the shell's signed-in accounts holds, its title resolved from what the machine can read; null = no title can be read (#EDE-194). */
export interface SpaceSession {
  id: string
  title?: string | null
  parentId?: string | null
  rootId?: string | null
}

export interface SpaceRow {
  id: string
  session: string
  number: number | null
  title: string
  subtitle: string | null
}

export interface Extent {
  width: number
  height: number
}

export interface SpaceDisplay extends Extent {
  scale: number
}

export interface SpaceView {
  width: number
  height: number
  picture: Extent
  pixels: Extent
  large: boolean
  canGrow: boolean
}

/** A space's number: the `n` of `space-<n>`, 1 to 15 digits (exact as a number). */
export function spaceNumber(id: string): number | null {
  const m = /^space-(\d{1,15})$/.exec(id)
  return m ? Number(m[1]) : null
}

/** Trimmed of white space as both twins read it (JavaScript's and Unicode's White_Space: U+0085 too). */
function trimmed(s: string): string {
  return s.replace(/^[\s\u0085]+|[\s\u0085]+$/g, ``)
}

function untitled(id: string, n: number | null): string {
  return fixture.copy.untitled.replaceAll(`{n}`, () => (n === null ? id : String(n)))
}

/** The list's rows in the order of the spaces' numbers. */
export function spaceRows(spaces: SpaceInput[], sessions: SpaceSession[]): SpaceRow[] {
  const byId = new Map(sessions.map((s) => [s.id, s]))
  const rows = spaces.map((space) => {
    const number = spaceNumber(space.id)
    const session = byId.get(space.session)
    const own = trimmed(session?.title ?? ``)
    const root = session?.parentId && session.rootId ? byId.get(session.rootId) : undefined
    const chat = trimmed(root?.title ?? ``)
    return { id: space.id, session: space.session, number, title: own || untitled(space.id, number), subtitle: chat || null }
  })
  return rows.sort((a, b) => {
    if (a.number !== null && b.number !== null) return a.number - b.number
    if (a.number !== null) return -1
    if (b.number !== null) return 1
    return a.id < b.id ? -1 : a.id > b.id ? 1 : 0
  })
}

/** The pinned view's box for a space of `space` on `display`. */
export function spaceView(space: Extent, display: SpaceDisplay, large: boolean): SpaceView {
  const { header, maxWidth, border } = fixture.constants
  const aspect = space.width > 0 && space.height > 0 ? space.height / space.width : 10 / 16
  const roomW = Math.max(display.width - 2 * LAYOUT.panelMargin, 0)
  const roomH = Math.max(display.height - LAYOUT.taskbarHeight - 2 * LAYOUT.panelMargin, 0)
  const pictureH = (w: number) => Math.round(w * aspect)
  const fits = (w: number) => header + pictureH(w) + 2 * border <= roomH
  let base = Math.min(maxWidth, Math.floor(display.width / 3))
  if (!fits(base)) base = Math.floor(Math.max(roomH - header - 2 * border, 0) / aspect)
  const canGrow = 2 * base + 2 * border <= roomW && fits(2 * base)
  const big = large && canGrow
  const width = big ? 2 * base : base
  const picture = { width, height: pictureH(width) }
  const device = (v: number, cap: number) => {
    const px = Math.round(v * display.scale)
    return cap > 0 ? Math.min(px, cap) : px
  }
  return { width: width + 2 * border, height: header + picture.height + 2 * border, picture, pixels: { width: device(picture.width, space.width), height: device(picture.height, space.height) }, large: big, canGrow }
}

/** #EDE-266: who drives a space (the compositor's `control`). */
export type SpaceControl = `agent` | `paused` | `person`

/** A trace line of the holder's session feed (`{title, icon, status}`). */
export interface TraceLine {
  title: string
  icon: string
  status: string
}

/** An entry of the holder's `sessions` feed: the chat `id`, its working sessions (`stop`) and their `trace`. */
export interface TraySession {
  id: string
  stop?: string[]
  trace?: TraceLine[]
}

export interface TrayInput {
  rows: SpaceRow[]
  /** The space the person picked; absent or gone = the first row. */
  selected?: string | null
  controls?: Record<string, SpaceControl>
  sessions?: TraySession[]
  /** The session whose view is pinned. */
  watching?: string | null
  /** #EDE-272: the sessions the person stopped (parked), oldest first. */
  stopped?: StoppedInput[]
}

/** A parked session as the shell resolved it (a row's title and chat). */
export interface StoppedInput {
  session: string
  title?: string | null
  subtitle?: string | null
}

export interface TrayStopped {
  session: string
  title: string
  subtitle: string | null
  allow: TrayAction
}

export interface TrayAction {
  label: string
  icon: string
  value: string
}

export interface TrayMenuItem {
  label?: string
  value?: string
  icon?: string
  kind?: `separator`
  destructive?: boolean
  detail?: string
}

export interface TraySelected {
  id: string
  session: string
  title: string
  subtitle: string | null
  control: SpaceControl
  state: { label: string; icon: string; variant: string }
  now: string | null
  trace: TraceLine[]
  watching: boolean
  watch: TrayAction
  pause: TrayAction | null
  take: TrayAction
}

export interface Tray {
  count: number
  shown: boolean
  badge: string
  list: boolean
  rows: (SpaceRow & { selected: boolean })[]
  selected: TraySelected | null
  menu: TrayMenuItem[]
  stopped: TrayStopped[]
}

/** #EDE-266: the computer-use tray's popup and applet menu. */
export function cuTray(input: TrayInput): Tray {
  const { copy, constants } = fixture
  const rows = input.rows
  const pick = rows.find((r) => r.id === input.selected) ?? rows[0]
  const count = rows.length
  const badge = count === 0 ? `` : count === 1 ? copy.spaceOne : copy.spaceMany.replaceAll(`{n}`, () => String(count))
  const action = (key: keyof typeof constants.trayIcons): TrayAction => ({ label: copy[key], icon: constants.trayIcons[key], value: key })
  let selected: TraySelected | null = null
  if (pick) {
    const control: SpaceControl = input.controls?.[pick.id] ?? `agent`
    const entry = (input.sessions ?? []).find((s) => s.id === pick.session || (s.stop ?? []).includes(pick.session))
    const trace = (entry?.trace ?? []).slice(-constants.trayTrace)
    const running = [...trace].reverse().find((line) => line.status === `running`)
    const watching = !!input.watching && input.watching === pick.session
    const state = { label: copy[control], icon: constants.trayIcons[control], variant: constants.trayVariants[control] }
    selected = {
      id: pick.id,
      session: pick.session,
      title: pick.title,
      subtitle: pick.subtitle,
      control,
      state,
      now: running ? running.title : null,
      trace,
      watching,
      watch: action(watching ? `unwatch` : `watch`),
      pause: control === `agent` ? action(`pause`) : control === `paused` ? action(`resume`) : null,
      take: action(control === `person` ? `handBack` : `takeOver`),
    }
  }
  const item = (a: TrayAction): TrayMenuItem => ({ label: a.label, value: a.value, icon: a.icon })
  const menu: TrayMenuItem[] = selected
    ? [item(selected.watch), ...(selected.pause ? [item(selected.pause)] : []), item(selected.take), { kind: `separator` }, { ...item(action(`stop`)), destructive: true }]
    : []
  // #EDE-272: the parked sessions, each with Allow computer use again.
  const stopped: TrayStopped[] = (input.stopped ?? []).map((s) => ({ session: s.session, title: trimmed(s.title ?? ``) || copy.stoppedUntitled, subtitle: s.subtitle ?? null, allow: action(`allow`) }))
  if (stopped.length > 0 && menu.length > 0) menu.push({ kind: `separator` })
  for (const s of stopped) menu.push({ label: s.allow.label, value: `allow:${s.session}`, icon: s.allow.icon, detail: s.title })
  return { count, shown: count > 0 || stopped.length > 0, badge, list: count > 1, rows: rows.map((r) => ({ ...r, selected: r.id === selected?.id })), selected, menu, stopped }
}
