// surfaces/board.tsx — issue-board primitives (status/priority/avatar/label/calendar),
// the 260px SidebarPane chassis, BoardTool (tinted status groups, 28px rows, cascade
// entrance, hover/selected, PR dot, FLIP regroup) and ReviewsTool (merge-button morph).
// Pixel truth: ref/desktop-hero-board-issue.png — sidebar chrome (#171717 header +
// pills) over a #0a0a0a issue list, tinted group bands, lucide-style glyphs, the
// fuchsia DS avatar, indigo "+ New Issue" button.
// All frame props are COMPOSITION-GLOBAL frames; every interpolation clamps.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, EASE, MONO_FONT, POP, UI_FONT, WIN } from "../theme"
import { REVIEW_ROW, type BoardRow, type IssueStatus, type Priority } from "../fixtures"
import { riseIn } from "../rig"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

// Avatar recipe sampled from ref/desktop-hero-board-issue.png (the DS circles):
// dark fuchsia fill ≈ #3d0f3a, brighter fuchsia ring, bright fuchsia initials.
// (Deliberately local — the shared theme has no fuchsia token; matched to the ref.)
const AVATAR_BG = `rgba(192,38,211,0.28)`
const AVATAR_RING = `rgba(217,70,239,0.55)`
const AVATAR_FG = `#e879f9`

const ROW_H = WIN.row // 28

// ── Tiny inline icons (lucide-style, stroke currentColor) ────────────────────
const svgProps = (size: number, strokeWidth = 1.6) =>
  ({
    width: size,
    height: size,
    viewBox: `0 0 24 24`,
    fill: `none`,
    stroke: `currentColor`,
    strokeWidth,
    strokeLinecap: `round`,
    strokeLinejoin: `round`,
  }) as const

const ChevronDownIcon: React.FC<{ size?: number }> = ({ size = 12 }) => (
  <svg {...svgProps(size, 2)}>
    <path d="m6 9 6 6 6-6" />
  </svg>
)

const CircleIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <circle cx="12" cy="12" r="9" />
  </svg>
)

const CircleDashedIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <circle cx="12" cy="12" r="9" strokeDasharray="3.6 3.4" />
  </svg>
)

const TimerIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <line x1="10" y1="2" x2="14" y2="2" />
    <line x1="12" y1="14" x2="15" y2="11" />
    <circle cx="12" cy="14" r="8" />
  </svg>
)

const CircleCheckIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <circle cx="12" cy="12" r="9" />
    <path d="m9 12 2 2 4-4" />
  </svg>
)

const MinusIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <path d="M5 12h14" />
  </svg>
)

const TriangleAlertIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 20h16a2 2 0 0 0 1.73-2Z" />
    <path d="M12 9v4" />
    <path d="M12 17h.01" />
  </svg>
)

// lucide signal-low / signal-medium / signal-high: baseline dot + 1/2/3 ascending bars.
const SignalIcon: React.FC<{ bars: 1 | 2 | 3; size?: number }> = ({ bars, size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <path d="M2 20h.01" />
    <path d="M7 20v-4" />
    {bars >= 2 ? <path d="M12 20v-8" /> : null}
    {bars >= 3 ? <path d="M17 20V8" /> : null}
  </svg>
)

const UserIcon: React.FC<{ size?: number }> = ({ size = 10 }) => (
  <svg {...svgProps(size, 2)}>
    <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" />
    <circle cx="12" cy="7" r="4" />
  </svg>
)

const ListFilterIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 1.8)}>
    <path d="M3 6h18" />
    <path d="M7 12h10" />
    <path d="M11 18h4" />
  </svg>
)

const PlusIcon: React.FC<{ size?: number }> = ({ size = 12 }) => (
  <svg {...svgProps(size, 2)}>
    <path d="M5 12h14" />
    <path d="M12 5v14" />
  </svg>
)

const GitPullRequestIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <svg {...svgProps(size, 2)}>
    <circle cx="18" cy="18" r="3" />
    <circle cx="6" cy="6" r="3" />
    <path d="M13 6h3a2 2 0 0 1 2 2v7" />
    <line x1="6" y1="9" x2="6" y2="21" />
  </svg>
)

// Exported per contract — the due-date calendar-days glyph on board rows.
export const CalendarGlyph: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 1.6)}>
    <path d="M8 2v4" />
    <path d="M16 2v4" />
    <rect x="3" y="4" width="18" height="18" rx="2" />
    <path d="M3 10h18" />
    <path d="M8 14h.01" />
    <path d="M12 14h.01" />
    <path d="M16 14h.01" />
    <path d="M8 18h.01" />
    <path d="M12 18h.01" />
  </svg>
)

// ── Primitives ────────────────────────────────────────────────────────────────
export const StatusIcon: React.FC<{ status: IssueStatus; size?: number }> = ({ status, size = 13 }) => {
  switch (status) {
    case `backlog`:
      return (
        <span style={{ color: C.statusBacklog, display: `flex` }}>
          <CircleDashedIcon size={size} />
        </span>
      )
    case `todo`:
      return (
        <span style={{ color: C.statusTodo, display: `flex` }}>
          <CircleIcon size={size} />
        </span>
      )
    case `in_progress`:
      return (
        <span style={{ color: C.statusInProgress, display: `flex` }}>
          <TimerIcon size={size} />
        </span>
      )
    case `done`:
      return (
        <span style={{ color: C.statusDone, display: `flex` }}>
          <CircleCheckIcon size={size} />
        </span>
      )
  }
}

export const PriorityIcon: React.FC<{ p: Priority; size?: number }> = ({ p, size = 13 }) => {
  switch (p) {
    case `none`:
      return (
        <span style={{ color: C.muted, display: `flex`, opacity: 0.8 }}>
          <MinusIcon size={size} />
        </span>
      )
    case `urgent`:
      return (
        <span style={{ color: C.prioUrgent, display: `flex` }}>
          <TriangleAlertIcon size={size} />
        </span>
      )
    case `high`:
      return (
        <span style={{ color: C.prioHigh, display: `flex` }}>
          <SignalIcon bars={3} size={size} />
        </span>
      )
    case `medium`:
      return (
        <span style={{ color: C.prioMedium, display: `flex` }}>
          <SignalIcon bars={2} size={size} />
        </span>
      )
    case `low`:
      return (
        <span style={{ color: C.prioLow, display: `flex` }}>
          <SignalIcon bars={1} size={size} />
        </span>
      )
  }
}

// initials undefined → the unassigned state (dashed ring + tiny muted user glyph).
export const Avatar: React.FC<{ initials?: string; size?: number }> = ({ initials, size = 18 }) => {
  if (!initials) {
    return (
      <span
        style={{
          width: size,
          height: size,
          flex: `none`,
          display: `inline-flex`,
          alignItems: `center`,
          justifyContent: `center`,
          borderRadius: 999,
          border: `1px dashed rgba(255,255,255,0.25)`,
          color: `rgba(250,250,250,0.35)`,
        }}
      >
        <UserIcon size={Math.round(size * 0.55)} />
      </span>
    )
  }
  return (
    <span
      style={{
        width: size,
        height: size,
        flex: `none`,
        display: `inline-flex`,
        alignItems: `center`,
        justifyContent: `center`,
        borderRadius: 999,
        backgroundColor: AVATAR_BG,
        border: `1px solid ${AVATAR_RING}`,
        color: AVATAR_FG,
        fontFamily: UI_FONT,
        fontSize: Math.round(size * 0.42),
        fontWeight: 600,
        lineHeight: 1,
        letterSpacing: 0.2,
      }}
    >
      {initials}
    </span>
  )
}

export const LabelChip: React.FC<{ name: string; dot: string }> = ({ name, dot }) => (
  <span
    style={{
      display: `inline-flex`,
      alignItems: `center`,
      gap: 4,
      height: 17,
      padding: `0 6px`,
      flex: `none`,
      borderRadius: 999,
      border: `1px solid ${C.borderSoft}`,
      fontFamily: UI_FONT,
      fontSize: 11,
      color: C.muted,
      whiteSpace: `nowrap`,
      maxWidth: 64,
      overflow: `hidden`,
    }}
  >
    <span style={{ width: 6, height: 6, flex: `none`, borderRadius: 999, backgroundColor: dot }} />
    <span style={{ overflow: `hidden`, textOverflow: `ellipsis` }}>{name}</span>
  </span>
)

// ── SidebarPane — the 260px tool-window chassis ──────────────────────────────
// Ref truth: the sidebar CHROME (title row + pill tabs) sits on #171717 (C.panel)
// with a right hairline; the issue list itself paints #0a0a0a (BoardTool does that).
// `pills` renders the board's filter pill row (true → the default three pills).

export type SidebarPills = { labels: string[]; activeIndex?: number } | boolean

// The board header's right cluster ("≡ Filter" ghost + indigo "+ New Issue").
export const BoardActions: React.FC = () => (
  <div style={{ display: `flex`, alignItems: `center`, gap: 8 }}>
    <span style={{ display: `inline-flex`, alignItems: `center`, gap: 4, color: C.muted, fontFamily: UI_FONT, fontSize: 12 }}>
      <ListFilterIcon size={13} />
      Filter
    </span>
    <span
      style={{
        display: `inline-flex`,
        alignItems: `center`,
        gap: 4,
        height: 24,
        padding: `0 8px`,
        borderRadius: 6,
        backgroundColor: C.indigo,
        color: `#ffffff`,
        fontFamily: UI_FONT,
        fontSize: 12,
        fontWeight: 500,
        whiteSpace: `nowrap`,
      }}
    >
      <PlusIcon size={11} />
      New Issue
    </span>
  </div>
)

export const SidebarPane: React.FC<{
  children: React.ReactNode
  title: string
  actions?: React.ReactNode
  pills?: SidebarPills
  bottomInset?: number // px kept free at the window bottom (animated dock height); default the collapsed strip
}> = ({ children, title, actions, pills, bottomInset = WIN.dockStrip }) => {
  const pillSpec =
    pills === true
      ? { labels: [`All Issues`, `Active`, `Backlog`], activeIndex: 0 }
      : pills === false || pills === undefined
        ? undefined
        : { labels: pills.labels, activeIndex: pills.activeIndex ?? 0 }
  return (
    <div
      style={{
        position: `absolute`,
        left: WIN.rail,
        top: WIN.topBar,
        width: WIN.sidebar,
        height: WIN.h - WIN.topBar - bottomInset,
        backgroundColor: C.panel,
        borderRight: `1px solid ${C.border}`,
        display: `flex`,
        flexDirection: `column`,
        fontFamily: UI_FONT,
        overflow: `hidden`,
      }}
    >
      <div
        style={{
          flex: `none`,
          height: 40,
          padding: `0 12px`,
          display: `flex`,
          alignItems: `center`,
          justifyContent: `space-between`,
        }}
      >
        <span style={{ fontSize: 13, fontWeight: 600, color: C.text }}>{title}</span>
        {actions ?? null}
      </div>
      {pillSpec ? (
        <div style={{ flex: `none`, display: `flex`, gap: 4, padding: `0 12px 8px` }}>
          {pillSpec.labels.map((label, i) => {
            const active = i === pillSpec.activeIndex
            return (
              <span
                key={label}
                style={{
                  height: 22,
                  padding: `0 10px`,
                  display: `inline-flex`,
                  alignItems: `center`,
                  borderRadius: 999,
                  backgroundColor: active ? C.accentBg : `transparent`,
                  color: active ? C.text : C.muted,
                  fontSize: 12,
                  fontWeight: active ? 500 : 400,
                  whiteSpace: `nowrap`,
                }}
              >
                {label}
              </span>
            )
          })}
        </div>
      ) : null}
      <div style={{ flex: 1, minHeight: 0, position: `relative` }}>{children}</div>
    </div>
  )
}

// ── BoardTool — grouped issue list ────────────────────────────────────────────

const GROUPS: { status: IssueStatus; label: string; tint: string }[] = [
  { status: `in_progress`, label: `In Progress`, tint: C.tintInProgress },
  { status: `todo`, label: `Todo`, tint: C.tintTodo },
  { status: `backlog`, label: `Backlog`, tint: C.tintBacklog },
  { status: `done`, label: `Done`, tint: C.tintDone },
]

type Placed = { y: number; index: number; count: number }

// Display layout: for each non-empty group (canonical order) a header then its
// rows (input order). Keys: `h:<status>` for headers, the row id for rows.
const computeLayout = (rows: BoardRow[]): Map<string, Placed> => {
  const map = new Map<string, Placed>()
  let y = 0
  let index = 0
  for (const g of GROUPS) {
    const members = rows.filter((r) => r.status === g.status)
    if (members.length === 0) continue
    map.set(`h:${g.status}`, { y, index, count: members.length })
    y += ROW_H
    index += 1
    for (const r of members) {
      map.set(r.id, { y, index, count: 0 })
      y += ROW_H
      index += 1
    }
  }
  return map
}

export type BoardHover = string | { id: string; from: number; to?: number }

export const BoardTool: React.FC<{
  frame: number
  rows: BoardRow[]
  overrides?: Record<string, Partial<BoardRow>>
  cascadeAt?: number // staggered entrance: 3f stagger per item, 9f fade + 12px rise
  hover?: BoardHover
  selectedId?: string // solid-accent selected row (assembler flashes it around the click)
  prDotId?: { id: string; at: number } // 6px green PR dot pops after the identifier
  regroup?: { id: string; t: number; from?: IssueStatus } // FLIP slide between groups; from = group being left (defaults to the row's base status)
  showLabels?: boolean // ref truth: the real 260px sidebar board hides label chips (titles win)
  insertAt?: { id: string; at: number } // row pops in at `at`: height 0→ROW_H + fade, rows below slide down
}> = ({ frame, rows, overrides, cascadeAt, hover, selectedId, prDotId, regroup, showLabels = true, insertAt }) => {
  const eff = rows.map((r) => ({ ...r, ...(overrides?.[r.id] ?? {}) }))
  const t = regroup ? Math.min(1, Math.max(0, regroup.t)) : 1
  const layoutB = computeLayout(eff)
  const layoutA = regroup
    ? computeLayout(
        eff.map((r) =>
          r.id === regroup.id
            ? { ...r, status: regroup.from ?? rows.find((b) => b.id === r.id)?.status ?? r.status }
            : r,
        ),
      )
    : layoutB

  // Insert pop: before `insertAt.at` the row is absent (layoutIns positions rule);
  // over INSERT_DUR frames it grows 0→ROW_H while rows below slide down.
  const INSERT_DUR = 12
  const tIns =
    insertAt === undefined
      ? 1
      : interpolate(frame, [insertAt.at, insertAt.at + INSERT_DUR], [0, 1], { ...CLAMP, easing: EASE })
  const layoutIns =
    insertAt === undefined || tIns >= 1 ? undefined : computeLayout(eff.filter((r) => r.id !== insertAt.id))

  const yOf = (key: string): number => {
    const b = layoutB.get(key)
    const a = layoutA.get(key)
    let y: number
    if (!b) y = a ? a.y : 0
    else if (!a || a.y === b.y) y = b.y
    else y = interpolate(t, [0, 1], [a.y, b.y], { ...CLAMP, easing: EASE })
    if (layoutIns !== undefined && insertAt !== undefined && key !== insertAt.id) {
      const pre = layoutIns.get(key)
      if (pre !== undefined && pre.y !== y) y = pre.y + (y - pre.y) * tIns
    }
    return y
  }

  const hoverOpacity = (id: string): number => {
    if (!hover) return 0
    if (typeof hover === `string`) return hover === id ? 1 : 0
    if (hover.id !== id) return 0
    const on = interpolate(frame, [hover.from, hover.from + 4], [0, 1], { ...CLAMP, easing: EASE })
    const off =
      hover.to === undefined ? 1 : interpolate(frame, [hover.to, hover.to + 4], [1, 0], { ...CLAMP, easing: EASE })
    return Math.min(on, off)
  }

  const enter = (index: number) =>
    cascadeAt === undefined ? { opacity: 1, translate: `0px 0px` } : riseIn(frame, cascadeAt + index * 3, 9, 12)

  const items: React.ReactNode[] = []

  for (const g of GROUPS) {
    const members = eff.filter((r) => r.status === g.status)
    if (members.length === 0) continue
    const headerKey = `h:${g.status}`
    const placedB = layoutB.get(headerKey)
    if (!placedB) continue
    const countA = layoutA.get(headerKey)?.count ?? placedB.count
    let count = t < 0.5 ? countA : placedB.count
    if (layoutIns !== undefined && tIns < 0.5) count = layoutIns.get(headerKey)?.count ?? count
    items.push(
      <div
        key={headerKey}
        style={{
          position: `absolute`,
          left: 0,
          right: 0,
          top: yOf(headerKey),
          height: ROW_H,
          display: `flex`,
          alignItems: `center`,
          gap: 6,
          padding: `0 10px`,
          backgroundColor: g.tint,
          borderBottom: `1px solid ${C.borderSoft}`,
          ...enter(placedB.index),
        }}
      >
        <span style={{ color: C.dim, display: `flex` }}>
          <ChevronDownIcon size={12} />
        </span>
        <StatusIcon status={g.status} size={13} />
        <span style={{ fontSize: 13, fontWeight: 600, color: C.text }}>{g.label}</span>
        <span style={{ fontSize: 11, color: C.muted }}>{count}</span>
      </div>,
    )

    for (const row of members) {
      const placedRow = layoutB.get(row.id)
      if (!placedRow) continue
      const isInserted = insertAt !== undefined && insertAt.id === row.id
      if (isInserted && frame < insertAt.at) continue
      const isMover = regroup !== undefined && regroup.id === row.id
      const inFlight = isMover && t > 0 && t < 1
      const hoverO = hoverOpacity(row.id)
      const selected = selectedId === row.id
      // In-flight accent tint peaks mid-slide (keeps the mover readable over rows it passes).
      const flightTint = inFlight ? 4 * t * (1 - t) * 0.45 : 0
      const dotOn = prDotId !== undefined && prDotId.id === row.id && frame >= prDotId.at
      const dotScale = dotOn
        ? spring({ frame: frame - (prDotId as { at: number }).at, fps: 30, config: POP })
        : 0
      // Status-glyph pop as the mover lands in its new group (t-driven, deterministic).
      const iconScale =
        isMover && t > 0 ? interpolate(t, [0, 0.18, 0.38], [0.4, 1.18, 1], CLAMP) : 1
      // Inserted-row entrance: height 0→ROW_H + fade, plus a soft indigo flash that decays.
      const insertFlash = isInserted
        ? interpolate(frame, [insertAt.at + 2, insertAt.at + 44], [0.22, 0], { ...CLAMP, easing: EASE })
        : 0
      const insertStyle: React.CSSProperties = isInserted
        ? { height: Math.max(0, ROW_H * tIns), overflow: `hidden`, opacity: tIns }
        : {}
      items.push(
        <div
          key={row.id}
          style={{
            position: `absolute`,
            left: 0,
            right: 0,
            top: yOf(row.id),
            height: ROW_H,
            display: `flex`,
            alignItems: `center`,
            gap: 6,
            padding: `0 10px`,
            borderBottom: `1px solid ${C.borderRow}`,
            backgroundColor: selected ? C.accentBg : inFlight ? C.bg : undefined,
            zIndex: inFlight ? 5 : undefined,
            boxShadow: inFlight ? `0 4px 16px rgba(0,0,0,${0.5 * 4 * t * (1 - t)})` : undefined,
            ...enter(placedRow.index),
            ...insertStyle,
          }}
        >
          {insertFlash > 0 ? (
            <div style={{ position: `absolute`, inset: 0, backgroundColor: `rgba(99,102,241,${insertFlash})` }} />
          ) : null}
          {hoverO > 0 && !selected ? (
            <div
              style={{
                position: `absolute`,
                inset: 0,
                backgroundColor: `rgba(38,38,38,${0.55 * hoverO})`,
              }}
            />
          ) : null}
          {flightTint > 0 ? (
            <div style={{ position: `absolute`, inset: 0, backgroundColor: `rgba(38,38,38,${flightTint})` }} />
          ) : null}
          <span style={{ width: 16, flex: `none`, display: `flex`, justifyContent: `center`, position: `relative` }}>
            <PriorityIcon p={row.priority} size={13} />
          </span>
          <span
            style={{
              width: 52,
              flex: `none`,
              display: `flex`,
              alignItems: `center`,
              gap: 3,
              fontFamily: MONO_FONT,
              fontSize: 11,
              color: C.muted,
              whiteSpace: `nowrap`,
              position: `relative`,
            }}
          >
            {row.id}
            {prDotId !== undefined && prDotId.id === row.id ? (
              <span
                style={{
                  width: 6,
                  height: 6,
                  flex: `none`,
                  borderRadius: 999,
                  backgroundColor: C.green,
                  scale: String(Math.max(0, dotScale)),
                }}
              />
            ) : null}
          </span>
          <span
            style={{
              width: 16,
              flex: `none`,
              display: `flex`,
              justifyContent: `center`,
              position: `relative`,
              scale: String(iconScale),
            }}
          >
            <StatusIcon status={row.status} size={13} />
          </span>
          <span
            style={{
              flex: 1,
              minWidth: 0,
              fontFamily: UI_FONT,
              fontSize: 13,
              color: C.text,
              whiteSpace: `nowrap`,
              overflow: `hidden`,
              textOverflow: `ellipsis`,
              position: `relative`,
            }}
          >
            {row.title}
          </span>
          {row.label && showLabels ? <LabelChip name={row.label.name} dot={row.label.dot} /> : null}
          <Avatar initials={row.assignee} size={18} />
          <span
            style={{
              flex: `none`,
              display: `flex`,
              color: C.muted,
              opacity: row.due ? 1 : 0.3,
              position: `relative`,
            }}
          >
            <CalendarGlyph size={13} />
          </span>
        </div>,
      )
    }
  }

  return (
    <div
      style={{
        position: `absolute`,
        inset: 0,
        backgroundColor: C.bg,
        overflow: `hidden`,
        fontFamily: UI_FONT,
      }}
    >
      {items}
    </div>
  )
}

// ── ReviewsTool — open-PR list with the two-stage merge button ────────────────

export type MergeState = `rest` | `confirm` | `merging` | `gone`

const MERGE_W: Record<Exclude<MergeState, `gone`>, number> = {
  rest: 54,
  confirm: 104,
  merging: 88,
}
const MERGE_PREV: Record<Exclude<MergeState, `gone`>, Exclude<MergeState, `gone`>> = {
  rest: `rest`,
  confirm: `rest`,
  merging: `confirm`,
}
const MERGE_LABEL: Record<Exclude<MergeState, `gone`>, string> = {
  rest: `Merge`,
  confirm: `Confirm merge`,
  merging: `Merging…`,
}

const Spinner: React.FC<{ frame: number; size?: number }> = ({ frame, size = 11 }) => (
  <span style={{ display: `flex`, rotate: `${(frame * 24) % 360}deg`, flex: `none` }}>
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round">
      <path d="M21 12a9 9 0 1 1-6.2-8.56" />
    </svg>
  </span>
)

export const ReviewsTool: React.FC<{
  frame: number
  mergeState: MergeState
  morphAt?: number // global frame the CURRENT mergeState began — drives the 6f width/color morph
  hover?: boolean // cursor over the merge button
  rowFade?: number // 0→1 row fade + height collapse (drive before/while switching to "gone")
  row?: { id: string; title: string; sub: string } // PR row content (default: the ships REVIEW_ROW)
  project?: string // group header project name (default "Exponential")
}> = ({ frame, mergeState, morphAt, hover, rowFade, row = REVIEW_ROW, project = `Exponential` }) => {
  const collapse = mergeState === `gone` ? 1 : Math.min(1, Math.max(0, rowFade ?? 0))
  const ROW_FULL = 48

  let button: React.ReactNode = null
  if (mergeState !== `gone`) {
    const morphT =
      morphAt === undefined ? 1 : interpolate(frame, [morphAt, morphAt + 6], [0, 1], { ...CLAMP, easing: EASE })
    const width = interpolate(morphT, [0, 1], [MERGE_W[MERGE_PREV[mergeState]], MERGE_W[mergeState]], CLAMP)
    const danger = mergeState === `confirm`
    const dangerO = danger ? morphT : 0
    const fg = danger ? C.destructive : mergeState === `merging` ? C.muted : C.text
    button = (
      <span
        style={{
          width,
          height: 22,
          flex: `none`,
          display: `inline-flex`,
          alignItems: `center`,
          justifyContent: `center`,
          gap: 5,
          borderRadius: 6,
          border: `1px solid ${danger ? `rgba(255,100,103,${0.35 + 0.35 * dangerO})` : C.input}`,
          backgroundColor: hover && mergeState === `rest` ? C.accentBg : `transparent`,
          color: fg,
          fontFamily: UI_FONT,
          fontSize: 12,
          fontWeight: 500,
          whiteSpace: `nowrap`,
          overflow: `hidden`,
        }}
      >
        {mergeState === `merging` ? <Spinner frame={frame} /> : null}
        {MERGE_LABEL[mergeState]}
      </span>
    )
  }

  return (
    <div style={{ position: `absolute`, inset: 0, fontFamily: UI_FONT, overflow: `hidden` }}>
      {/* group header: project dot + name */}
      <div style={{ height: ROW_H, display: `flex`, alignItems: `center`, gap: 8, padding: `0 12px` }}>
        <span style={{ width: 8, height: 8, flex: `none`, borderRadius: 999, backgroundColor: C.indigoSoft }} />
        <span style={{ fontSize: 12, fontWeight: 600, color: C.muted }}>{project}</span>
      </div>
      {/* the one PR row (collapses via rowFade / "gone") */}
      <div style={{ height: ROW_FULL * (1 - collapse), opacity: 1 - collapse, overflow: `hidden` }}>
        <div style={{ margin: `0 8px`, padding: `5px 6px`, borderRadius: 6 }}>
          <div style={{ display: `flex`, alignItems: `center`, gap: 6 }}>
            <span style={{ color: C.green, display: `flex`, flex: `none` }}>
              <GitPullRequestIcon size={14} />
            </span>
            <span style={{ fontFamily: MONO_FONT, fontSize: 12, color: C.muted, flex: `none` }}>{row.id}</span>
            <span
              style={{
                flex: 1,
                minWidth: 0,
                fontSize: 13,
                color: C.text,
                whiteSpace: `nowrap`,
                overflow: `hidden`,
                textOverflow: `ellipsis`,
              }}
            >
              {row.title}
            </span>
            {button}
          </div>
          <div
            style={{
              paddingLeft: 20,
              marginTop: 2,
              fontFamily: MONO_FONT,
              fontSize: 11,
              color: C.muted,
              whiteSpace: `nowrap`,
              overflow: `hidden`,
              textOverflow: `ellipsis`,
            }}
          >
            {row.sub}
          </div>
        </div>
      </div>
    </div>
  )
}
