// surfaces/boardlist.tsx — the board's issue list as the MAIN view of the
// cutout panel (crates/ui/src/issue_list.rs; shots/board/desktop.webp). The
// sidebar never lists a board's issues any more: the board is a screen.
// Authored in PANEL-LOCAL LOGICAL px (render inside shell.tsx CutoutPanel).
//
// Geometry off the shot (shot px / 1.25): 28px group bands tinted with the
// status accent at 7%, 28px rows with a faint hairline; columns = priority
// (center 39), mono identifier (x 50), status glyph (center 132), title
// (x 150, text_sm); right-aligned label pill (24px glass capsule, colour dot
// + name) · the 16px assignee circle (dashed placeholder when unassigned) ·
// the due date (calendar glyph + "Jul 19").

import React from "react"
import { interpolate } from "remotion"
import { CalendarDays, ChevronDown, User } from "lucide-react"
import { C, EASE, MONO_FONT, UI_FONT } from "../theme"
import type { BoardRow, IssueStatus } from "../fixtures"
import { PriorityIcon } from "./board"
import { G, Ico, StatusGlyph, T } from "./shell"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const ROW = 28

const GROUPS: { status: IssueStatus; label: string; accent: string }[] = [
  { status: `backlog`, label: `Backlog`, accent: `161,161,170` },
  { status: `in_progress`, label: `In Progress`, accent: `234,179,8` },
  { status: `in_review`, label: `In Review`, accent: `34,197,94` },
  { status: `done`, label: `Done`, accent: `59,130,246` },
]

type Placed = { y: number; count: number }
const layout = (rows: BoardRow[]): Map<string, Placed> => {
  const map = new Map<string, Placed>()
  let y = 0
  for (const g of GROUPS) {
    const members = rows.filter((r) => r.status === g.status)
    if (members.length === 0) continue
    map.set(`h:${g.status}`, { y, count: members.length })
    y += ROW
    for (const r of members) {
      map.set(r.id, { y, count: 0 })
      y += ROW
    }
  }
  return map
}

/** Panel-local logical center of a row's title cell (cursor aim). */
export const boardRowCenter = (
  rows: BoardRow[],
  id: string
): { x: number; y: number } => ({
  x: 260,
  y: (layout(rows).get(id)?.y ?? 0) + ROW / 2,
})

// Avatar tints (the app derives each member's circle from their identity).
const AVATAR: Record<string, { bg: string; fg: string }> = {
  RC: { bg: `rgba(234,179,8,0.22)`, fg: `#facc15` },
  JL: { bg: `rgba(20,184,166,0.25)`, fg: `#5eead4` },
  MK: { bg: `rgba(249,115,22,0.25)`, fg: `#fb923c` },
}

export const MiniAvatar: React.FC<{ initials?: string; size?: number }> = ({
  initials,
  size = 16,
}) => {
  if (!initials)
    return (
      <span
        style={{
          width: size,
          height: size,
          flex: `none`,
          borderRadius: 999,
          border: `1px dashed rgba(255,255,255,0.22)`,
          boxSizing: `border-box`,
          display: `flex`,
          alignItems: `center`,
          justifyContent: `center`,
          color: `rgba(250,250,250,0.4)`,
        }}
      >
        <Ico I={User} size={size * 0.5} />
      </span>
    )
  const a = AVATAR[initials] ?? AVATAR.RC
  return (
    <span
      style={{
        width: size,
        height: size,
        flex: `none`,
        borderRadius: 999,
        backgroundColor: a.bg,
        color: a.fg,
        fontSize: size * 0.42,
        fontWeight: 600,
        display: `flex`,
        alignItems: `center`,
        justifyContent: `center`,
        letterSpacing: 0.1,
      }}
    >
      {initials}
    </span>
  )
}

export const LabelPill: React.FC<{ name: string; dot: string }> = ({
  name,
  dot,
}) => (
  <span
    style={{
      height: 24,
      boxSizing: `border-box`,
      display: `flex`,
      alignItems: `center`,
      gap: 5,
      padding: `0 9px`,
      borderRadius: 999,
      border: `1px solid ${G.strokeCard}`,
      backgroundColor: G.fillCard,
      fontSize: T.xs,
      color: G.fg,
      whiteSpace: `nowrap`,
      flex: `none`,
    }}
  >
    <span style={{ width: 6, height: 6, borderRadius: 999, backgroundColor: dot }} />
    {name}
  </span>
)

export type BoardListProps = {
  frame: number
  rows: BoardRow[]
  overrides?: Record<string, Partial<BoardRow>>
  /** FLIP: the row slides from its `from` group to its current one, t 0→1 */
  regroup?: { id: string; t: number; from: IssueStatus }
  /** soft white pulse on a row (a teammate's live edit) */
  flashAt?: { id: string; at: number }
  hover?: { id: string; from: number; to?: number }
  width: number
}

export const BoardList: React.FC<BoardListProps> = ({
  frame,
  rows,
  overrides,
  regroup,
  flashAt,
  hover,
  width,
}) => {
  const eff = rows.map((r) => ({ ...r, ...(overrides?.[r.id] ?? {}) }))
  const t = regroup ? Math.min(1, Math.max(0, regroup.t)) : 1
  const B = layout(eff)
  const A = regroup
    ? layout(eff.map((r) => (r.id === regroup.id ? { ...r, status: regroup.from } : r)))
    : B
  const yOf = (key: string): number => {
    const b = B.get(key)
    const a = A.get(key)
    if (!b) return a?.y ?? 0
    if (!a || a.y === b.y) return b.y
    return interpolate(t, [0, 1], [a.y, b.y], { ...CLAMP, easing: EASE })
  }
  const items: React.ReactNode[] = []
  for (const g of GROUPS) {
    const pb = B.get(`h:${g.status}`)
    const pa = A.get(`h:${g.status}`)
    if (!pb && !pa) continue
    const count = t < 0.5 ? (pa?.count ?? 0) : (pb?.count ?? 0)
    const headerO = pb ? 1 : 1 - t
    items.push(
      <div
        key={`h:${g.status}`}
        style={{
          position: `absolute`,
          left: 0,
          right: 0,
          top: yOf(`h:${g.status}`),
          height: ROW,
          boxSizing: `border-box`,
          display: `flex`,
          alignItems: `center`,
          backgroundColor: `rgba(${g.accent},0.07)`,
          borderBottom: `1px solid ${G.strokeRow}`,
          opacity: headerO,
        }}
      >
        <span style={{ position: `absolute`, left: 21.6 - 5, color: G.muted, display: `flex` }}>
          <Ico I={ChevronDown} size={10} />
        </span>
        <span style={{ position: `absolute`, left: 44.8 - 6, display: `flex` }}>
          <StatusGlyph status={g.status} size={12} />
        </span>
        <span
          style={{
            position: `absolute`,
            left: 56,
            display: `flex`,
            alignItems: `baseline`,
            gap: 6,
            whiteSpace: `nowrap`,
          }}
        >
          <span style={{ fontSize: T.sm, fontWeight: 600 }}>{g.label}</span>
          <span style={{ fontSize: T.xs, color: G.muted }}>{count}</span>
        </span>
      </div>
    )
    for (const r of eff.filter((x) => x.status === g.status)) {
      const mover = regroup?.id === r.id
      const inFlight = mover && t > 0 && t < 1
      const flash =
        flashAt?.id === r.id && frame >= flashAt.at
          ? interpolate(frame, [flashAt.at, flashAt.at + 44], [0.2, 0], {
              ...CLAMP,
              easing: EASE,
            })
          : 0
      const hov =
        hover?.id === r.id
          ? Math.min(
              interpolate(frame, [hover.from, hover.from + 4], [0, 1], CLAMP),
              hover.to === undefined
                ? 1
                : interpolate(frame, [hover.to, hover.to + 4], [1, 0], CLAMP)
            )
          : 0
      const iconScale = mover && t > 0 ? interpolate(t, [0, 0.2, 0.4], [0.5, 1.15, 1], CLAMP) : 1
      items.push(
        <div
          key={r.id}
          style={{
            position: `absolute`,
            left: 0,
            right: 0,
            top: yOf(r.id),
            height: ROW,
            boxSizing: `border-box`,
            borderBottom: `1px solid rgba(255,255,255,0.025)`,
            backgroundColor: inFlight ? `#1b1b1e` : undefined,
            boxShadow: inFlight
              ? `0 4px 16px rgba(0,0,0,${0.5 * 4 * t * (1 - t)})`
              : undefined,
            zIndex: inFlight ? 5 : undefined,
          }}
        >
          {flash + 0.05 * hov > 0 ? (
            <div
              style={{
                position: `absolute`,
                inset: 0,
                backgroundColor: `rgba(255,255,255,${flash + 0.05 * hov})`,
              }}
            />
          ) : null}
          <span style={{ position: `absolute`, left: 39.2 - 6, top: 8, display: `flex` }}>
            <PriorityIcon p={r.priority} size={13} />
          </span>
          <span
            style={{
              position: `absolute`,
              left: 50.4,
              top: 0,
              height: ROW,
              display: `flex`,
              alignItems: `center`,
              fontFamily: MONO_FONT,
              fontSize: 11,
              color: G.muted,
            }}
          >
            {r.id}
          </span>
          <span
            style={{
              position: `absolute`,
              left: 132 - 6,
              top: 8,
              display: `flex`,
              scale: String(iconScale),
            }}
          >
            <StatusGlyph status={r.status} size={12} />
          </span>
          <span
            style={{
              position: `absolute`,
              left: 150.4,
              right: 320,
              top: 0,
              height: ROW,
              display: `flex`,
              alignItems: `center`,
              fontFamily: UI_FONT,
              fontSize: T.sm,
              fontWeight: 500,
              color: G.fg,
              whiteSpace: `nowrap`,
              overflow: `hidden`,
              textOverflow: `ellipsis`,
            }}
          >
            {r.title}
          </span>
          <div
            style={{
              position: `absolute`,
              right: 46,
              top: 0,
              height: ROW,
              display: `flex`,
              alignItems: `center`,
              gap: 8,
            }}
          >
            {r.label ? <LabelPill name={r.label.name} dot={r.label.dot} /> : null}
            <span style={{ marginLeft: 3, display: `flex` }}>
              <MiniAvatar initials={r.assignee} />
            </span>
            {r.due ? (
              <span
                style={{
                  display: `flex`,
                  alignItems: `center`,
                  gap: 4,
                  marginLeft: 3,
                  fontSize: T.xs,
                  color: G.muted,
                  whiteSpace: `nowrap`,
                }}
              >
                <Ico I={CalendarDays} size={11} />
                {r.due}
              </span>
            ) : null}
          </div>
        </div>
      )
    }
  }
  return (
    <div
      style={{
        position: `absolute`,
        left: 0,
        top: 0,
        width,
        bottom: 0,
        overflow: `hidden`,
        fontFamily: UI_FONT,
        color: C.text,
      }}
    >
      {items}
    </div>
  )
}
