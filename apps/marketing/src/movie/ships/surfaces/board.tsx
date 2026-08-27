// surfaces/board.tsx — issue-board primitives (status/priority/avatar/label/calendar),
// the 520px SidebarPane tool-window chassis, BoardTool (tinted status groups,
// 28px rows, cascade entrance, hover/selected, PR dot, FLIP regroup) and
// ReviewsTool (merge-button morph).
// Pixel truth (EXP-359 glass): the real-app reference screenshot + desktop
// crates/ui/src/issue_list.rs — the whole pane is TRANSPARENT over the page
// gradient; rows hover FILL_ROW / select FILL_ACTIVE; group headers keep a
// status tint at 10% alpha (between the two white washes); done is BLUE.
// All frame props are COMPOSITION-GLOBAL frames; every interpolation clamps.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, EASE, MONO_FONT, POP, R, UI_FONT, WIN } from "../theme"
import {
  REVIEW_ROW,
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
    case `todo`:
      return (
        <span style={{ color: C.statusTodo, display: `flex` }}>
          <CircleIcon size={size} />
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
  { status: `todo`, label: `Todo`, tint: C.tintTodo },
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

// ── ReviewsTool — open-PR list with the two-stage merge button ────────────────
// EXP-471 pixel truth: shots/reviews/desktop.webp + desktop
// crates/ui/src/sidebar.rs `render_reviews_tool` / `review_row` (EXP-642).
// The pane is a 30px tool header (git-pull-request glyph + muted "Reviews"),
// then a p-2 list: one board group row (dot + name + count) over GAPPED glass
// row CARDS — one per open PR. Card line 1 is the green PR glyph, the mono
// identifier, the title, a ghost `×` (close without merging) and the outlined
// capsule Merge button (web `sm`: h-8 px-3 rounded-full, merge glyph + label);
// line 2 is the mono `#N · branch` at pl-5. The card whose diff is open in the
// center wears the ACTIVE fill.

export type MergeState = `rest` | `confirm` | `merging` | `gone`

// Web `sm` button metrics (controls.rs web_sm: h-8, px-3, capsule) — widths are
// the measured label boxes plus the 12px paddings, so the morph is a real
// width interpolation between the three labels.
const MERGE_H = 28
const MERGE_W: Record<Exclude<MergeState, `gone`>, number> = {
  rest: 74, // ⑂ + "Merge"
  confirm: 134, // "Confirm merge" (danger, no glyph)
  merging: 106, // spinner + "Merging…"
}
const MERGE_PREV: Record<
  Exclude<MergeState, `gone`>,
  Exclude<MergeState, `gone`>
> = {
  rest: `rest`,
  confirm: `rest`,
  merging: `confirm`,
}
const MERGE_LABEL: Record<Exclude<MergeState, `gone`>, string> = {
  rest: `Merge`,
  confirm: `Confirm merge`,
  merging: `Merging…`,
}

// Reviews pane geometry (window-local, used by the segments' cursor keys):
// 30px tool header, an 8px list inset, the group row, then the card.
export const REVIEWS_HEADER_H = 30
const LIST_PAD = 8
const GROUP_H = 22
const CARD_GAP = 8
const CARD_PAD_X = 12
const CARD_PAD_Y = 10
const CARD_SUB_H = 15
// 1px stroke + padding + line 1 (the button box) + 2px gap + the sub line.
const CARD_H = 2 * (1 + CARD_PAD_Y) + MERGE_H + 2 + CARD_SUB_H
// The card's top edge, measured from the pane's own top (below the titlebar).
const CARD_TOP = REVIEWS_HEADER_H + LIST_PAD + GROUP_H + CARD_GAP

// lucide git-merge — the glyph the Merge button carries (registry PR_MERGED).
const GitMergeIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <circle cx="18" cy="18" r="3" />
    <circle cx="6" cy="6" r="3" />
    <path d="M6 21V9a9 9 0 0 0 9 9" />
  </svg>
)

// lucide x — the quiet reject affordance (close the PR without merging).
const XIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <path d="M18 6 6 18" />
    <path d="m6 6 12 12" />
  </svg>
)

const Spinner: React.FC<{ frame: number; size?: number }> = ({
  frame,
  size = 11,
}) => (
  <span
    style={{
      display: `flex`,
      rotate: `${(frame * 24) % 360}deg`,
      flex: `none`,
    }}
  >
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2.4}
      strokeLinecap="round"
    >
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
}> = ({
  frame,
  mergeState,
  morphAt,
  hover,
  rowFade,
  row = REVIEW_ROW,
  project = `Exponential`,
}) => {
  const collapse =
    mergeState === `gone` ? 1 : Math.min(1, Math.max(0, rowFade ?? 0))

  let button: React.ReactNode = null
  if (mergeState !== `gone`) {
    const morphT =
      morphAt === undefined
        ? 1
        : interpolate(frame, [morphAt, morphAt + 6], [0, 1], {
            ...CLAMP,
            easing: EASE,
          })
    const width = interpolate(
      morphT,
      [0, 1],
      [MERGE_W[MERGE_PREV[mergeState]], MERGE_W[mergeState]],
      CLAMP
    )
    const danger = mergeState === `confirm`
    const dangerO = danger ? morphT : 0
    const fg = danger
      ? C.destructive
      : mergeState === `merging`
        ? C.muted
        : C.text
    button = (
      <span
        style={{
          width,
          height: MERGE_H,
          flex: `none`,
          display: `inline-flex`,
          alignItems: `center`,
          justifyContent: `center`,
          gap: 6,
          borderRadius: 999,
          border: `1px solid ${danger ? `rgba(255,100,103,${0.35 + 0.35 * dangerO})` : C.strokeStrong}`,
          backgroundColor:
            hover && mergeState === `rest` ? C.fillActive : `transparent`,
          color: fg,
          fontFamily: UI_FONT,
          fontSize: 12,
          fontWeight: 500,
          whiteSpace: `nowrap`,
          overflow: `hidden`,
        }}
      >
        {mergeState === `merging` ? <Spinner frame={frame} /> : null}
        {mergeState === `rest` ? <GitMergeIcon size={13} /> : null}
        {MERGE_LABEL[mergeState]}
      </span>
    )
  }

  return (
    <div
      style={{
        position: `absolute`,
        inset: 0,
        fontFamily: UI_FONT,
        overflow: `hidden`,
      }}
    >
      {/* tool header: git-pull-request glyph + the muted tool name */}
      <div
        style={{
          height: REVIEWS_HEADER_H,
          display: `flex`,
          alignItems: `center`,
          gap: 6,
          padding: `0 12px`,
          color: `rgba(250,250,250,0.7)`,
        }}
      >
        <GitPullRequestIcon size={13} />
        <span style={{ fontSize: 12, fontWeight: 500 }}>Reviews</span>
      </div>
      <div
        style={{
          padding: LIST_PAD,
          display: `flex`,
          flexDirection: `column`,
          gap: CARD_GAP,
        }}
      >
        {/* group header: board dot + name + open-PR count. It goes with the
            last card — the real list renders no empty groups. */}
        <div
          style={{
            height: GROUP_H,
            display: `flex`,
            alignItems: `center`,
            gap: 6,
            padding: `0 4px`,
            opacity: 1 - collapse,
          }}
        >
          <span
            style={{
              width: 8,
              height: 8,
              flex: `none`,
              borderRadius: 999,
              backgroundColor: C.neutral,
            }}
          />
          <span
            style={{
              fontSize: 13,
              fontWeight: 500,
              color: `rgba(250,250,250,0.7)`,
            }}
          >
            {project}
          </span>
          <span style={{ fontSize: 11, color: `rgba(250,250,250,0.5)` }}>1</span>
        </div>
        {/* the one PR card (collapses via rowFade / "gone") */}
        <div
          style={{
            height: CARD_H * (1 - collapse),
            opacity: 1 - collapse,
            overflow: `hidden`,
          }}
        >
          <div
            style={{
              height: CARD_H,
              boxSizing: `border-box`,
              padding: `${CARD_PAD_Y}px ${CARD_PAD_X}px`,
              borderRadius: R.row,
              border: `1px solid ${C.strokeRow}`,
              // this PR's diff is the open center screen — the active fill
              backgroundColor: C.fillActive,
            }}
          >
            <div
              style={{
                height: MERGE_H,
                display: `flex`,
                alignItems: `center`,
                gap: 6,
              }}
            >
              <span style={{ color: C.green, display: `flex`, flex: `none` }}>
                <GitPullRequestIcon size={13} />
              </span>
              <span
                style={{
                  fontFamily: MONO_FONT,
                  fontSize: 12,
                  color: C.muted,
                  flex: `none`,
                }}
              >
                {row.id}
              </span>
              <span
                style={{
                  flex: 1,
                  minWidth: 0,
                  fontSize: 13,
                  fontWeight: 500,
                  color: C.text,
                  whiteSpace: `nowrap`,
                  overflow: `hidden`,
                  textOverflow: `ellipsis`,
                }}
              >
                {row.title}
              </span>
              <span
                style={{
                  width: 22,
                  height: 22,
                  flex: `none`,
                  display: `inline-flex`,
                  alignItems: `center`,
                  justifyContent: `center`,
                  borderRadius: 999,
                  color: C.muted,
                }}
              >
                <XIcon size={13} />
              </span>
              {button}
            </div>
            <div
              style={{
                height: CARD_SUB_H,
                paddingLeft: 20,
                marginTop: 2,
                fontFamily: MONO_FONT,
                fontSize: 11,
                lineHeight: `${CARD_SUB_H}px`,
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
    </div>
  )
}

// The Merge button's center in WINDOW-LOCAL coordinates (the space the
// segments' cursor keys live in) — the cursor has to land ON the button, so
// the number is derived from the card metrics instead of eyeballed.
// `CONTENT_TOP` is the pane's top (the titlebar's height); the pane's right
// edge is the rail + the sidebar width.
export const reviewsMergeCenter = (
  state: Exclude<MergeState, `gone`> = `rest`
): { x: number; y: number } => ({
  x: WIN.rail + WIN.sidebar - LIST_PAD - 1 - CARD_PAD_X - MERGE_W[state] / 2,
  y: WIN.titleBar + CARD_TOP + 1 + CARD_PAD_Y + MERGE_H / 2,
})

// ── ActionsTool (the Actions rail surface, EXP-385: merge → deploy) ──────────
// The saved-runbook list; one row's Run button morphs Run → Running… with the
// merge-button grammar while the deploy session streams in the dock.

const ZapGlyph: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
    style={{ display: `block`, flexShrink: 0 }}
  >
    <path d="M4 14a1 1 0 0 1-.78-1.63l9.9-10.2a.5.5 0 0 1 .86.46l-1.92 6.02A1 1 0 0 0 13 10h7a1 1 0 0 1 .78 1.63l-9.9 10.2a.5.5 0 0 1-.86-.46l1.92-6.02A1 1 0 0 0 11 14z" />
  </svg>
)

export type ActionRow = { id: string; name: string; sub: string }
export type ActionRunState = `rest` | `running`

const RUN_W: Record<ActionRunState, number> = { rest: 48, running: 96 }

export const ActionsTool: React.FC<{
  frame: number
  rows: readonly ActionRow[]
  runId: string // the action being run
  hoverAt?: number // global frame the cursor reaches its Run button
  runAt?: number // global frame of the Run click → "Running…" morph
  team?: string // group header team name (default "Exponential")
}> = ({ frame, rows, runId, hoverAt, runAt, team = `Exponential` }) => {
  const running = runAt !== undefined && frame >= runAt
  const morphT =
    runAt === undefined
      ? 0
      : interpolate(frame, [runAt, runAt + 6], [0, 1], {
          ...CLAMP,
          easing: EASE,
        })
  const hovering =
    hoverAt !== undefined && frame >= hoverAt && (runAt === undefined || frame < runAt)

  return (
    <div
      style={{
        position: `absolute`,
        inset: 0,
        fontFamily: UI_FONT,
        overflow: `hidden`,
      }}
    >
      {/* group header: team dot + name */}
      <div
        style={{
          height: ROW_H,
          display: `flex`,
          alignItems: `center`,
          gap: 8,
          padding: `0 12px`,
        }}
      >
        <span
          style={{
            width: 8,
            height: 8,
            flex: `none`,
            borderRadius: 999,
            backgroundColor: C.neutral,
          }}
        />
        <span style={{ fontSize: 12, fontWeight: 600, color: C.muted }}>
          {team}
        </span>
      </div>
      {rows.map((row) => {
        const isTarget = row.id === runId
        const state: ActionRunState = isTarget && running ? `running` : `rest`
        const width = isTarget
          ? interpolate(morphT, [0, 1], [RUN_W.rest, RUN_W[state]], CLAMP)
          : RUN_W.rest
        return (
          <div key={row.id} style={{ margin: `0 8px`, padding: `5px 6px` }}>
            <div style={{ display: `flex`, alignItems: `center`, gap: 6 }}>
              <span style={{ color: C.muted, display: `flex`, flex: `none` }}>
                <ZapGlyph size={14} />
              </span>
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
                {row.name}
              </span>
              <span
                style={{
                  width,
                  height: 22,
                  flex: `none`,
                  display: `inline-flex`,
                  alignItems: `center`,
                  justifyContent: `center`,
                  gap: 5,
                  borderRadius: 8,
                  border: `1px solid ${C.strokeStrong}`,
                  backgroundColor:
                    isTarget && hovering ? C.fillActive : `transparent`,
                  color: state === `running` ? C.muted : C.text,
                  fontFamily: UI_FONT,
                  fontSize: 12,
                  fontWeight: 500,
                  whiteSpace: `nowrap`,
                  overflow: `hidden`,
                }}
              >
                {state === `running` ? <Spinner frame={frame} /> : null}
                {state === `running` ? `Running…` : `Run`}
              </span>
            </div>
            <div
              style={{
                paddingLeft: 20,
                marginTop: 2,
                fontSize: 11.5,
                color: C.muted,
                whiteSpace: `nowrap`,
                overflow: `hidden`,
                textOverflow: `ellipsis`,
              }}
            >
              {row.sub}
            </div>
          </div>
        )
      })}
    </div>
  )
}
