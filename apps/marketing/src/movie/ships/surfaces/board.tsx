// surfaces/board.tsx — issue-board primitives (status/priority/avatar/label/calendar),
// the 520px SidebarPane tool-window chassis and BoardTool (tinted status
// groups, 28px rows, cascade entrance, hover/selected, PR dot, FLIP regroup).
// EXP-706 retired the docked Reviews TOOL window (and EXP-686 the Actions
// one): both are tab-less full-page screens now, so neither has a surface
// here — the PR diff's own header carries the two-stage merge (diffview.tsx).
// Pixel truth (EXP-359 glass): the real-app reference screenshot + desktop
// crates/ui/src/issue_list.rs — the whole pane is TRANSPARENT over the page
// gradient; rows hover FILL_ROW / select FILL_ACTIVE; group headers keep a
// status tint at 10% alpha (between the two white washes); done is BLUE.
// All frame props are COMPOSITION-GLOBAL frames; every interpolation clamps.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, EASE, MONO_FONT, POP, UI_FONT, WIN } from "../theme"
import {
  type BoardRow,
  type IssueStatus,
  type Priority,
} from "../fixtures"
import { riseIn } from "../rig"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

// Avatar recipe sampled from the desktop-hero-board-issue reference screenshot (the DS circles):
// dark fuchsia fill ≈ #3d0f3a, brighter fuchsia ring, bright fuchsia initials.
// (Deliberately local — the shared theme has no fuchsia token; matched to the ref.)
// The app tints each member's circle from their identity; the reference board
// shows blue / amber / green / emerald side by side, so the film derives the
// accent from the initials instead of painting everyone fuchsia.
const AVATAR_ACCENTS = [
  { bg: `rgba(59,130,246,0.30)`, fg: `#93c5fd` },
  { bg: `rgba(245,158,11,0.30)`, fg: `#fcd34d` },
  { bg: `rgba(34,197,94,0.30)`, fg: `#86efac` },
  { bg: `rgba(217,70,239,0.28)`, fg: `#e879f9` },
  { bg: `rgba(20,184,166,0.30)`, fg: `#5eead4` },
] as const

const avatarAccent = (initials: string) => {
  let h = 0
  for (const ch of initials) h = (h * 31 + ch.charCodeAt(0)) % 997
  return AVATAR_ACCENTS[h % AVATAR_ACCENTS.length]
}

const ROW_H = WIN.row // 28
// The list pane's own header strip (the Filter row) — pane content starts here.
export const HEADER_H = 44

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

const CircleDashedIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <circle cx="12" cy="12" r="9" strokeDasharray="3.6 3.4" />
  </svg>
)

// Pie-clock started glyph (icons.json custom progress-2-4 — the builtin
// In Progress icon since EXP-314): ring + half wedge.
const PieClockIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <circle cx="12" cy="12" r="10" />
    <path
      d="M12 12 L12 6 A6 6 0 0 1 12 18 Z"
      fill="currentColor"
      stroke="none"
    />
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
const SignalIcon: React.FC<{ bars: 1 | 2 | 3; size?: number }> = ({
  bars,
  size = 13,
}) => (
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
export const StatusIcon: React.FC<{ status: IssueStatus; size?: number }> = ({
  status,
  size = 13,
}) => {
  switch (status) {
    case `backlog`:
      return (
        <span style={{ color: C.statusBacklog, display: `flex` }}>
          <CircleDashedIcon size={size} />
        </span>
      )
    case `in_progress`:
      return (
        <span style={{ color: C.statusInProgress, display: `flex` }}>
          <PieClockIcon size={size} />
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

export const PriorityIcon: React.FC<{ p: Priority; size?: number }> = ({
  p,
  size = 13,
}) => {
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
export const Avatar: React.FC<{ initials?: string; size?: number }> = ({
  initials,
  size = 18,
}) => {
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
  const accent = avatarAccent(initials)
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
        backgroundColor: accent.bg,
        color: accent.fg,
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

export const LabelChip: React.FC<{ name: string; dot: string }> = ({
  name,
  dot,
}) => (
  <span
    style={{
      display: `inline-flex`,
      alignItems: `center`,
      gap: 5,
      flex: `none`,
      fontFamily: UI_FONT,
      fontSize: 12,
      color: C.muted,
      whiteSpace: `nowrap`,
      maxWidth: 92,
      overflow: `hidden`,
    }}
  >
    <span
      style={{
        width: 6,
        height: 6,
        flex: `none`,
        borderRadius: 999,
        backgroundColor: dot,
      }}
    />
    <span style={{ overflow: `hidden`, textOverflow: `ellipsis` }}>
      {name.charAt(0).toUpperCase() + name.slice(1)}
    </span>
  </span>
)

// ── SidebarPane — the 520px tool-window chassis ──────────────────────────────
// EXP-359 glass: the whole pane is transparent over the page gradient; a
// STROKE_ROW hairline marks the boundary to the center (surface.rs idiom).
// The board pane has NO title row and NO tab strip (EXP-282) — just the ghost
// Filter button in a 44px header strip.

// The board header's right cluster. Post-EXP-282 the list pane header carries
// ONE ghost "Filter" button — the primary "+ New Issue" moved to the titlebar
// and the All Issues / Active / Backlog tab strip is gone (shots/board/desktop).
export const BoardActions: React.FC = () => (
  <div
    style={{
      display: `inline-flex`,
      alignItems: `center`,
      gap: 5,
      height: 22,
      padding: `0 8px`,
      borderRadius: 8,
      color: C.muted,
      fontFamily: UI_FONT,
      fontSize: 12,
    }}
  >
    <ListFilterIcon size={13} />
    Filter
  </div>
)

export const SidebarPane: React.FC<{
  children: React.ReactNode
  title?: string // legacy label for non-board tools (Reviews); the board pane has none
  actions?: React.ReactNode
  bottomInset?: number // px kept free at the window bottom (animated dock height); default the collapsed strip
}> = ({ children, title, actions, bottomInset = WIN.dockStrip }) => (
  <div
    style={{
      position: `absolute`,
      left: WIN.rail,
      top: WIN.titleBar,
      width: WIN.sidebar,
      height: WIN.h - WIN.titleBar - bottomInset,
      borderRight: `1px solid ${C.strokeRow}`,
      display: `flex`,
      flexDirection: `column`,
      fontFamily: UI_FONT,
      overflow: `hidden`,
    }}
  >
    {/* The header strip only exists for panes that put something in it — the
        tool windows that draw their own 30px `tool_header` (Reviews) start at
        the pane's top edge instead. */}
    {title || actions ? (
      <div
        style={{
          flex: `none`,
          height: HEADER_H,
          padding: `0 14px`,
          display: `flex`,
          alignItems: `center`,
          justifyContent: `space-between`,
        }}
      >
        {title ? (
          <span style={{ fontSize: 13, fontWeight: 600, color: C.text }}>
            {title}
          </span>
        ) : (
          <span />
        )}
        {actions ?? null}
      </div>
    ) : null}
    <div style={{ flex: 1, minHeight: 0, position: `relative` }}>{children}</div>
  </div>
)

// ── BoardTool — grouped issue list ────────────────────────────────────────────

// Contract displayOrder (issueStatusDefaults): backlog → unstarted → started
// → completed. The app groups the board in exactly this order.
const GROUPS: { status: IssueStatus; label: string; tint: string }[] = [
  { status: `backlog`, label: `Backlog`, tint: C.tintBacklog },
  { status: `in_progress`, label: `In Progress`, tint: C.tintInProgress },
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
  flashAt?: { id: string; at: number } // soft white pulse on a row at `at` (a teammate's live edit, EXP-337)
}> = ({
  frame,
  rows,
  overrides,
  cascadeAt,
  hover,
  selectedId,
  prDotId,
  regroup,
  showLabels = true,
  insertAt,
  flashAt,
}) => {
  const eff = rows.map((r) => ({ ...r, ...(overrides?.[r.id] ?? {}) }))
  const t = regroup ? Math.min(1, Math.max(0, regroup.t)) : 1
  const layoutB = computeLayout(eff)
  const layoutA = regroup
    ? computeLayout(
        eff.map((r) =>
          r.id === regroup.id
            ? {
                ...r,
                status:
                  regroup.from ??
                  rows.find((b) => b.id === r.id)?.status ??
                  r.status,
              }
            : r
        )
      )
    : layoutB

  // Insert pop: before `insertAt.at` the row is absent (layoutIns positions rule);
  // over INSERT_DUR frames it grows 0→ROW_H while rows below slide down.
  const INSERT_DUR = 12
  const tIns =
    insertAt === undefined
      ? 1
      : interpolate(frame, [insertAt.at, insertAt.at + INSERT_DUR], [0, 1], {
          ...CLAMP,
          easing: EASE,
        })
  const layoutIns =
    insertAt === undefined || tIns >= 1
      ? undefined
      : computeLayout(eff.filter((r) => r.id !== insertAt.id))

  const yOf = (key: string): number => {
    const b = layoutB.get(key)
    const a = layoutA.get(key)
    let y: number
    if (!b) y = a ? a.y : 0
    else if (!a || a.y === b.y) y = b.y
    else y = interpolate(t, [0, 1], [a.y, b.y], { ...CLAMP, easing: EASE })
    if (
      layoutIns !== undefined &&
      insertAt !== undefined &&
      key !== insertAt.id
    ) {
      const pre = layoutIns.get(key)
      if (pre !== undefined && pre.y !== y) y = pre.y + (y - pre.y) * tIns
    }
    return y
  }

  const hoverOpacity = (id: string): number => {
    if (!hover) return 0
    if (typeof hover === `string`) return hover === id ? 1 : 0
    if (hover.id !== id) return 0
    const on = interpolate(frame, [hover.from, hover.from + 4], [0, 1], {
      ...CLAMP,
      easing: EASE,
    })
    const off =
      hover.to === undefined
        ? 1
        : interpolate(frame, [hover.to, hover.to + 4], [1, 0], {
            ...CLAMP,
            easing: EASE,
          })
    return Math.min(on, off)
  }

  const enter = (index: number) =>
    cascadeAt === undefined
      ? { opacity: 1, translate: `0px 0px` }
      : riseIn(frame, cascadeAt + index * 3, 9, 12)

  const items: React.ReactNode[] = []

  for (const g of GROUPS) {
    const members = eff.filter((r) => r.status === g.status)
    if (members.length === 0) continue
    const headerKey = `h:${g.status}`
    const placedB = layoutB.get(headerKey)
    if (!placedB) continue
    const countA = layoutA.get(headerKey)?.count ?? placedB.count
    let count = t < 0.5 ? countA : placedB.count
    if (layoutIns !== undefined && tIns < 0.5)
      count = layoutIns.get(headerKey)?.count ?? count
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
          gap: 5,
          padding: `0 11px`,
          backgroundColor: g.tint,
          ...enter(placedB.index),
        }}
      >
        <span style={{ color: C.dim, display: `flex`, marginRight: 1 }}>
          <ChevronDownIcon size={12} />
        </span>
        <StatusIcon status={g.status} size={13} />
        <span
          style={{
            fontSize: 13,
            fontWeight: 600,
            color: C.text,
            marginLeft: 1,
          }}
        >
          {g.label}
        </span>
        <span style={{ fontSize: 11.5, color: C.muted, marginLeft: 1 }}>
          {count}
        </span>
      </div>
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
      const dotOn =
        prDotId !== undefined && prDotId.id === row.id && frame >= prDotId.at
      const dotScale = dotOn
        ? spring({
            frame: frame - (prDotId as { at: number }).at,
            fps: 30,
            config: POP,
          })
        : 0
      // Status-glyph pop as the mover lands in its new group (t-driven, deterministic).
      const iconScale =
        isMover && t > 0
          ? interpolate(t, [0, 0.18, 0.38], [0.4, 1.18, 1], CLAMP)
          : 1
      // Inserted-row entrance: height 0→ROW_H + fade, plus a soft white flash that decays.
      const liveFlash =
        flashAt !== undefined && flashAt.id === row.id && frame >= flashAt.at
          ? interpolate(frame, [flashAt.at, flashAt.at + 44], [0.22, 0], {
              ...CLAMP,
              easing: EASE,
            })
          : 0
      const insertFlash = isInserted
        ? interpolate(frame, [insertAt.at + 2, insertAt.at + 44], [0.22, 0], {
            ...CLAMP,
            easing: EASE,
          })
        : liveFlash
      const insertStyle: React.CSSProperties = isInserted
        ? {
            height: Math.max(0, ROW_H * tIns),
            overflow: `hidden`,
            opacity: tIns,
          }
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
            padding: `0 11px 0 24px`,
            backgroundColor: selected
              ? C.fillActive
              : inFlight
                ? C.bgBottom
                : undefined,
            zIndex: inFlight ? 5 : undefined,
            boxShadow: inFlight
              ? `0 4px 16px rgba(0,0,0,${0.5 * 4 * t * (1 - t)})`
              : undefined,
            ...enter(placedRow.index),
            ...insertStyle,
          }}
        >
          {insertFlash > 0 ? (
            <div
              style={{
                position: `absolute`,
                inset: 0,
                backgroundColor: `rgba(255,255,255,${insertFlash})`,
              }}
            />
          ) : null}
          {hoverO > 0 && !selected ? (
            <div
              style={{
                position: `absolute`,
                inset: 0,
                backgroundColor: `rgba(255,255,255,${0.05 * hoverO})`,
              }}
            />
          ) : null}
          {flightTint > 0 ? (
            <div
              style={{
                position: `absolute`,
                inset: 0,
                backgroundColor: `rgba(255,255,255,${0.28 * flightTint})`,
              }}
            />
          ) : null}
          <span
            style={{
              width: 16,
              flex: `none`,
              display: `flex`,
              justifyContent: `center`,
              position: `relative`,
            }}
          >
            <PriorityIcon p={row.priority} size={13} />
          </span>
          <span
            style={{
              width: 68,
              flex: `none`,
              display: `flex`,
              alignItems: `center`,
              gap: 3,
              fontFamily: MONO_FONT,
              fontSize: 11.5,
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
          {row.label && showLabels ? (
            <LabelChip name={row.label.name} dot={row.label.dot} />
          ) : null}
          <Avatar initials={row.assignee} size={18} />
          {row.due ? (
            <span
              style={{
                flex: `none`,
                display: `flex`,
                alignItems: `center`,
                gap: 4,
                color: C.muted,
                fontSize: 12,
                whiteSpace: `nowrap`,
                position: `relative`,
              }}
            >
              <CalendarGlyph size={13} />
              {row.due}
            </span>
          ) : null}
        </div>
      )
    }
  }

  return (
    <div
      style={{
        position: `absolute`,
        inset: 0,
        overflow: `hidden`,
        fontFamily: UI_FONT,
      }}
    >
      {items}
    </div>
  )
}
