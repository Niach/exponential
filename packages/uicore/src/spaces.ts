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
