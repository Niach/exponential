// closedloop/surfaces/mobileui.tsx — the REAL mobile app screens (EXP-388).
// Every phone shot in the film renders one of these, mirrored off the shipping
// iOS/Android UI (IssueListView / IssueDetailView / MobileTabBar): the grouped
// board list with glass rows and the floating icon-only glass tab bar + the
// detached compose circle, and the issue detail with its identifier pill,
// property chip box and the icon-only play circle in the floating bottom bar.
// Nothing invented: no filter-chip strips, no text labels under tab icons, no
// visible "Start coding" label on the detail screen.
// All frame props are LOCAL to the segment that renders the surface.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, MONO_FONT, POP, UI_FONT } from "../../ships/theme"
import { ExpLogo } from "../../ships/rig"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

export const Glyph: React.FC<{
  size: number
  sw?: number
  style?: React.CSSProperties
  children: React.ReactNode
}> = ({ size, sw = 1.9, style, children }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={sw}
    strokeLinecap="round"
    strokeLinejoin="round"
    style={{ display: "block", flexShrink: 0, ...style }}
  >
    {children}
  </svg>
)

// ── Status + priority glyphs (contract vocabulary, theme tokens) ─────────────
export type MobileStatus = "backlog" | "todo" | "in_progress" | "done"

export const statusColor = (s: MobileStatus): string =>
  s === "backlog"
    ? C.statusBacklog
    : s === "todo"
      ? C.statusTodo
      : s === "in_progress"
        ? C.statusInProgress
        : C.statusDone

export const MStatusIcon: React.FC<{ status: MobileStatus; size: number }> = ({
  status,
  size,
}) => (
  <span style={{ color: statusColor(status), display: "flex" }}>
    {status === "backlog" ? (
      <Glyph size={size} sw={2}>
        <circle cx="12" cy="12" r="9" strokeDasharray="3.6 3.4" />
      </Glyph>
    ) : status === "todo" ? (
      <Glyph size={size} sw={2}>
        <circle cx="12" cy="12" r="9" />
      </Glyph>
    ) : status === "in_progress" ? (
      <Glyph size={size} sw={2}>
        <circle cx="12" cy="12" r="10" />
        <path
          d="M12 12 L12 6 A6 6 0 0 1 12 18 Z"
          fill="currentColor"
          stroke="none"
        />
      </Glyph>
    ) : (
      // Done: filled blue disc with a cut-out check (matches the app).
      <Glyph size={size} sw={2}>
        <circle cx="12" cy="12" r="10" fill="currentColor" stroke="none" />
        <path d="m8.5 12 2.5 2.5 5-5" stroke="#0b0b0e" />
      </Glyph>
    )}
  </span>
)

export type MobilePriority = "none" | "urgent" | "high" | "medium" | "low"

const prioColor = (p: MobilePriority): string =>
  p === "urgent"
    ? C.prioUrgent
    : p === "high"
      ? C.prioHigh
      : p === "medium"
        ? C.prioMedium
        : p === "low"
          ? C.prioLow
          : C.muted

// Signal bars with a per-level active count (low 1 · medium 2 · high 3),
// inactive bars ghosted — like the app. Urgent is the alert triangle.
const PRIO_ACTIVE: Record<"low" | "medium" | "high", number> = {
  low: 1,
  medium: 2,
  high: 3,
}

export const MPriorityIcon: React.FC<{
  priority: MobilePriority
  size: number
}> = ({ priority, size }) => (
  <span style={{ color: prioColor(priority), display: "flex" }}>
    {priority === "none" ? (
      <Glyph size={size} sw={2}>
        <path d="M5 12h14" />
      </Glyph>
    ) : priority === "urgent" ? (
      <Glyph size={size} sw={2}>
        <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" />
        <path d="M12 9v4" />
        <path d="M12 17h.01" />
      </Glyph>
    ) : (
      <Glyph size={size} sw={2.6}>
        {(["M5 20v-4", "M12 20v-9", "M19 20V6"] as const).map((d, i) => (
          <path key={d} d={d} opacity={i < PRIO_ACTIVE[priority] ? 1 : 0.25} />
        ))}
      </Glyph>
    )}
  </span>
)

// ── The floating glass tab bar (icon-only) + detached compose circle ─────────
// Real MobileTabBar: Issues (list) · My Work (inbox) · Support (life-buoy,
// helpdesk teams) · Agents (bot) · Reviews (git-pull-request) · Search — no
// text labels; active = white glyph on a white-12% circle; the square-pen
// compose circle floats detached right.
const TAB_ICONS: { id: string; node: React.ReactNode }[] = [
  {
    id: "issues",
    node: (
      <Glyph size={15}>
        <path d="M8 6h13" />
        <path d="M8 12h13" />
        <path d="M8 18h13" />
        <path d="M3 6h.01" />
        <path d="M3 12h.01" />
        <path d="M3 18h.01" />
      </Glyph>
    ),
  },
  {
    id: "mywork",
    node: (
      <Glyph size={15}>
        <path d="M22 12h-6l-2 3h-4l-2-3H2" />
        <path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />
      </Glyph>
    ),
  },
  {
    id: "support",
    node: (
      <Glyph size={15}>
        <circle cx="12" cy="12" r="10" />
        <circle cx="12" cy="12" r="4" />
        <path d="m4.93 4.93 4.24 4.24" />
        <path d="m14.83 14.83 4.24 4.24" />
        <path d="m14.83 9.17 4.24-4.24" />
        <path d="m4.93 19.07 4.24-4.24" />
      </Glyph>
    ),
  },
  {
    id: "agents",
    node: (
      <Glyph size={15}>
        <path d="M12 8V4H8" />
        <rect x="4" y="8" width="16" height="12" rx="2" />
        <path d="M2 14h2" />
        <path d="M20 14h2" />
        <path d="M15 13v2" />
        <path d="M9 13v2" />
      </Glyph>
    ),
  },
  {
    id: "reviews",
    node: (
      <Glyph size={15}>
        <circle cx="6" cy="6" r="3" />
        <circle cx="18" cy="18" r="3" />
        <path d="M13 6h3a2 2 0 0 1 2 2v7" />
        <path d="M6 9v12" />
      </Glyph>
    ),
  },
  {
    id: "search",
    node: (
      <Glyph size={15}>
        <circle cx="11" cy="11" r="8" />
        <path d="m21 21-4.3-4.3" />
      </Glyph>
    ),
  },
]

export const MobileTabBar: React.FC<{
  active?: string
  dots?: readonly string[] // tab ids carrying an unread/running dot
}> = ({ active = "issues", dots = [] }) => (
  <>
    <div
      style={{
        position: "absolute",
        left: 12,
        bottom: 14,
        height: 46,
        padding: "0 5px",
        borderRadius: 999,
        backgroundColor: "rgba(23,23,23,0.94)",
        border: `1px solid ${C.strokeCard}`,
        boxShadow: "0 10px 30px rgba(0,0,0,0.5)",
        display: "flex",
        alignItems: "center",
        gap: 1,
      }}
    >
      {TAB_ICONS.map(({ id, node }) => (
        <span
          key={id}
          style={{
            position: "relative",
            width: 34,
            height: 34,
            borderRadius: 999,
            backgroundColor: id === active ? C.fillActive : "transparent",
            color: id === active ? C.text : C.muted,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          {node}
          {dots.includes(id) ? (
            <span
              style={{
                position: "absolute",
                top: 5,
                right: 6,
                width: 6,
                height: 6,
                borderRadius: 999,
                backgroundColor: id === "agents" ? C.green : C.primary,
              }}
            />
          ) : null}
        </span>
      ))}
    </div>
    {/* detached compose circle (square-pen) */}
    <span
      style={{
        position: "absolute",
        right: 12,
        bottom: 14,
        width: 46,
        height: 46,
        borderRadius: 999,
        backgroundColor: C.fillActive,
        border: `1px solid ${C.strokeCard}`,
        color: C.text,
        boxShadow: "0 10px 30px rgba(0,0,0,0.5)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <Glyph size={16} sw={1.9}>
        <path d="M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
        <path d="M18.375 2.625a2.121 2.121 0 1 1 3 3L12 15l-4 1 1-4Z" />
      </Glyph>
    </span>
  </>
)

// ── Push banner (real notification copy rides in via props) ──────────────────
export const MobilePushBanner: React.FC<{
  frame: number
  at: number
  title: string
  body: string
}> = ({ frame, at, title, body }) => {
  if (frame < at) return null
  const t = spring({ frame: frame - at, fps: 30, config: POP })
  return (
    <div
      style={{
        position: "absolute",
        left: 10,
        right: 10,
        top: 46,
        boxSizing: "border-box",
        borderRadius: 18,
        padding: "10px 12px",
        backgroundColor: C.panelFloat,
        border: `1px solid ${C.strokeCard}`,
        boxShadow: "0 14px 34px rgba(0,0,0,0.45)",
        display: "flex",
        gap: 10,
        opacity: Math.min(1, t * 2),
        translate: `0px ${(t - 1) * 34}px`,
        zIndex: 6,
      }}
    >
      <div style={{ flexShrink: 0, marginTop: 1 }}>
        <ExpLogo size={26} />
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div
          style={{
            display: "flex",
            alignItems: "baseline",
            justifyContent: "space-between",
            gap: 8,
          }}
        >
          <span
            style={{
              fontSize: 11.5,
              fontWeight: 600,
              color: C.text,
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
            }}
          >
            {title}
          </span>
          <span style={{ fontSize: 10.5, color: C.dim, flexShrink: 0 }}>
            now
          </span>
        </div>
        <div
          style={{
            marginTop: 2,
            fontSize: 11.5,
            lineHeight: 1.4,
            color: C.muted,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {body}
        </div>
      </div>
    </div>
  )
}

// ── The board screen (grouped issue list, IssueListView twin) ────────────────
export type PhoneBoardRow = {
  id: string
  title: string
  priority: MobilePriority
  status: MobileStatus
  labelDot?: string
  assignee?: string
}

const SECTION_ORDER: { status: MobileStatus; name: string }[] = [
  { status: "in_progress", name: "In Progress" },
  { status: "todo", name: "Todo" },
  { status: "backlog", name: "Backlog" },
  { status: "done", name: "Done" },
]

const LIST_TOP = 84
const HEADER_H = 26
const ROW_H = 40
const ROW_GAP = 4

type LaidOut = { key: string; y: number }

// Stack section headers + rows top-down for a given per-row status map.
const layout = (
  rows: readonly PhoneBoardRow[],
  statusOf: (row: PhoneBoardRow) => MobileStatus
): { els: LaidOut[]; counts: Record<string, number> } => {
  const els: LaidOut[] = []
  const counts: Record<string, number> = {}
  let y = LIST_TOP
  for (const section of SECTION_ORDER) {
    const inSection = rows.filter((row) => statusOf(row) === section.status)
    if (inSection.length === 0) continue
    counts[section.status] = inSection.length
    els.push({ key: `h:${section.status}`, y })
    y += HEADER_H
    for (const row of inSection) {
      els.push({ key: row.id, y })
      y += ROW_H + ROW_GAP
    }
    y += 4
  }
  return { els, counts }
}

export type BoardScreenProps = {
  frame: number
  boardName: string
  rows: readonly PhoneBoardRow[]
  /** After-state status overrides; moveT lerps every element between layouts. */
  overrides?: Record<string, MobileStatus>
  moveT?: number
  /** Row that exists only in the after layout — pops in as moveT rises. */
  insertId?: string
  activeTab?: string
  tabDots?: readonly string[]
  banner?: { at: number; title: string; body: string }
}

export const BoardScreen: React.FC<BoardScreenProps> = ({
  frame,
  boardName,
  rows,
  overrides = {},
  moveT = 0,
  insertId,
  activeTab = "issues",
  tabDots = [],
  banner,
}) => {
  const before = rows.filter((row) => row.id !== insertId)
  const after = rows
  const statusBefore = (row: PhoneBoardRow) => row.status
  const statusAfter = (row: PhoneBoardRow) => overrides[row.id] ?? row.status

  const layoutB = layout(before, statusBefore)
  const layoutA = layout(after, statusAfter)
  const t = moveT
  const live = t >= 0.5 ? layoutA : layoutB
  const yB = new Map(layoutB.els.map((el) => [el.key, el.y]))
  const yA = new Map(layoutA.els.map((el) => [el.key, el.y]))

  const posOf = (key: string): { y: number; o: number } => {
    const b = yB.get(key)
    const a = yA.get(key)
    if (b === undefined && a === undefined) return { y: -999, o: 0 }
    if (b === undefined) return { y: a as number, o: t } // enters with the move
    if (a === undefined) return { y: b, o: 1 - t } // leaves with the move
    return { y: b + (a - b) * t, o: 1 }
  }

  const rowById = new Map(rows.map((row) => [row.id, row]))

  return (
    <div style={{ position: "absolute", inset: 0, fontFamily: UI_FONT }}>
      {/* nav bar: board name + chevrons-up-down combobox, filter + gear right */}
      <div
        style={{
          position: "absolute",
          top: 44,
          left: 0,
          right: 0,
          height: 30,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          gap: 6,
        }}
      >
        <span style={{ fontSize: 16, fontWeight: 600, color: C.text }}>
          {boardName}
        </span>
        <span style={{ color: C.dim, display: "flex" }}>
          <Glyph size={11} sw={2.4}>
            <path d="m7 15 5 5 5-5" />
            <path d="m7 9 5-5 5 5" />
          </Glyph>
        </span>
      </div>
      {/* filter + settings share ONE trailing glass capsule (like the app) */}
      <div
        style={{
          position: "absolute",
          top: 42,
          right: 12,
          height: 34,
          boxSizing: "border-box",
          display: "flex",
          alignItems: "center",
          gap: 14,
          padding: "0 14px",
          borderRadius: 999,
          backgroundColor: C.fillCard,
          border: `1px solid rgba(255,255,255,0.06)`,
          color: C.muted,
        }}
      >
        <Glyph size={15} sw={2}>
          <path d="M10 5h11" />
          <path d="M13 12h8" />
          <path d="M16 19h5" />
          <path d="M3 5h.01" />
          <path d="M6 12h.01" />
          <path d="M9 19h.01" />
        </Glyph>
        <Glyph size={15} sw={1.7}>
          <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
          <circle cx="12" cy="12" r="3" />
        </Glyph>
      </div>

      {/* grouped list */}
      {SECTION_ORDER.map((section) => {
        const pos = posOf(`h:${section.status}`)
        if (pos.o <= 0.01) return null
        const count = live.counts[section.status] ?? 0
        return (
          <div
            key={section.status}
            style={{
              position: "absolute",
              left: 16,
              right: 16,
              top: pos.y,
              height: HEADER_H,
              display: "flex",
              alignItems: "center",
              gap: 7,
              opacity: pos.o,
            }}
          >
            <span style={{ color: C.dim, display: "flex" }}>
              <Glyph size={10} sw={2.4}>
                <path d="m6 9 6 6 6-6" />
              </Glyph>
            </span>
            <MStatusIcon status={section.status} size={13} />
            <span
              style={{
                fontSize: 13,
                fontWeight: 600,
                color: "rgba(255,255,255,0.85)",
              }}
            >
              {section.name}
            </span>
            <span style={{ fontSize: 11.5, color: C.dim }}>{count}</span>
          </div>
        )
      })}
      {rows.map((row) => {
        const pos = posOf(row.id)
        if (pos.o <= 0.01) return null
        const data = rowById.get(row.id) as PhoneBoardRow
        const status = t >= 0.5 ? (overrides[row.id] ?? row.status) : row.status
        return (
          <div
            key={row.id}
            style={{
              position: "absolute",
              left: 12,
              right: 12,
              top: pos.y,
              height: ROW_H,
              boxSizing: "border-box",
              display: "flex",
              alignItems: "center",
              gap: 7,
              padding: "0 10px",
              borderRadius: 10,
              backgroundColor: C.fillCard,
              border: `1px solid rgba(255,255,255,0.06)`,
              opacity: pos.o,
            }}
          >
            <MPriorityIcon priority={data.priority} size={12} />
            <span
              style={{
                fontFamily: MONO_FONT,
                fontSize: 10,
                color: C.dim,
                flexShrink: 0,
              }}
            >
              {data.id}
            </span>
            <MStatusIcon status={status} size={12} />
            <span
              style={{
                fontSize: 12.5,
                color: C.text,
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
                flex: 1,
                minWidth: 0,
              }}
            >
              {data.title}
            </span>
            {data.labelDot ? (
              <span
                style={{
                  width: 8,
                  height: 8,
                  borderRadius: 999,
                  backgroundColor: data.labelDot,
                  flexShrink: 0,
                }}
              />
            ) : null}
            {data.assignee ? (
              <span
                style={{
                  width: 20,
                  height: 20,
                  borderRadius: 999,
                  backgroundColor: C.fillActive,
                  color: C.text,
                  fontSize: 9.5,
                  fontWeight: 600,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  flexShrink: 0,
                }}
              >
                {data.assignee.slice(0, 1)}
              </span>
            ) : null}
            <span style={{ color: C.dim, display: "flex" }}>
              <Glyph size={11} sw={2.2}>
                <path d="m9 18 6-6-6-6" />
              </Glyph>
            </span>
          </div>
        )
      })}

      <MobileTabBar active={activeTab} dots={tabDots} />
      {banner ? (
        <MobilePushBanner
          frame={frame}
          at={banner.at}
          title={banner.title}
          body={banner.body}
        />
      ) : null}
    </div>
  )
}

// ── The issue detail screen (IssueDetailView twin) ───────────────────────────
export type IssueScreenProps = {
  frame: number
  identifier: string
  title: string
  origin?: string // "Feedback widget" origin chip next to the identifier pill
  status: MobileStatus
  statusLabel: string
  priorityLabel: string
  labelChip?: { name: string; dot: string }
  description: string
  /** Coding/PR status rows (hairline rows, not a box); ready → merged at mergedAt. */
  pr?: { number: number; device: string; user: string; mergedAt?: number }
  /** Activity timeline lines under the rail dots (e.g. "… created the issue · 1 hr ago"). */
  activity?: readonly string[]
  /** Press flash on the play circle (the icon-only start button). */
  playPressAt?: number
  /** Replaces the play glyph with a pulsing green dot (session live). */
  sessionLive?: boolean
}

export const IssueScreen: React.FC<IssueScreenProps> = ({
  frame,
  identifier,
  title,
  origin,
  status,
  statusLabel,
  priorityLabel,
  labelChip,
  description,
  pr,
  activity,
  playPressAt,
  sessionLive,
}) => {
  const pressT =
    playPressAt === undefined
      ? 0
      : interpolate(
          frame,
          [playPressAt, playPressAt + 3, playPressAt + 9],
          [0, 1, 0],
          CLAMP
        )
  const merged = pr?.mergedAt !== undefined && frame >= pr.mergedAt
  const shownStatus: MobileStatus = merged ? "done" : status
  const shownStatusLabel = merged ? "Done" : statusLabel

  const chip = (children: React.ReactNode, key?: string) => (
    <span
      key={key}
      style={{
        height: 26,
        boxSizing: "border-box",
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        padding: "0 10px",
        borderRadius: 999,
        backgroundColor: C.fillCard,
        border: `1px solid rgba(255,255,255,0.08)`,
        fontSize: 11.5,
        fontWeight: 500,
        color: C.text,
      }}
    >
      {children}
    </span>
  )

  return (
    <div style={{ position: "absolute", inset: 0, fontFamily: UI_FONT }}>
      {/* nav bar: circular glass back button · "Issue" · circular … menu */}
      <div
        style={{
          position: "absolute",
          top: 40,
          left: 12,
          right: 12,
          height: 34,
          display: "flex",
          alignItems: "center",
          color: C.muted,
        }}
      >
        <span
          style={{
            width: 34,
            height: 34,
            borderRadius: 999,
            backgroundColor: C.fillCard,
            border: `1px solid rgba(255,255,255,0.06)`,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <Glyph size={15} sw={2.2}>
            <path d="m15 18-6-6 6-6" />
          </Glyph>
        </span>
        <span
          style={{
            position: "absolute",
            left: 0,
            right: 0,
            textAlign: "center",
            fontSize: 15,
            fontWeight: 600,
            color: C.text,
          }}
        >
          Issue
        </span>
        <span
          style={{
            marginLeft: "auto",
            width: 34,
            height: 34,
            borderRadius: 999,
            backgroundColor: C.fillCard,
            border: `1px solid rgba(255,255,255,0.06)`,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <Glyph size={15} sw={2}>
            <circle cx="5" cy="12" r="0.9" fill="currentColor" />
            <circle cx="12" cy="12" r="0.9" fill="currentColor" />
            <circle cx="19" cy="12" r="0.9" fill="currentColor" />
          </Glyph>
        </span>
      </div>

      {/* scrolling column: identifier pill → title → chip box → description
          → coding/PR card (flow layout — chips may wrap like the real app) */}
      <div
        style={{
          position: "absolute",
          top: 86,
          left: 16,
          right: 16,
          display: "flex",
          flexDirection: "column",
          gap: 14,
          alignItems: "flex-start",
        }}
      >
        <div style={{ display: "flex", gap: 6 }}>
          <span
            style={{
              height: 22,
              display: "inline-flex",
              alignItems: "center",
              padding: "0 9px",
              borderRadius: 999,
              backgroundColor: C.fillCard,
              border: `1px solid rgba(255,255,255,0.08)`,
              fontFamily: MONO_FONT,
              fontSize: 10.5,
              fontWeight: 500,
              color: C.muted,
            }}
          >
            {identifier}
          </span>
          {origin ? (
            <span
              style={{
                height: 22,
                display: "inline-flex",
                alignItems: "center",
                gap: 5,
                padding: "0 9px",
                borderRadius: 999,
                backgroundColor: C.fillCard,
                border: `1px solid rgba(255,255,255,0.08)`,
                fontSize: 10.5,
                color: C.muted,
              }}
            >
              <Glyph size={10} sw={2}>
                <path d="m3 11 18-5v12L3 14v-3z" />
                <path d="M11.6 16.8a3 3 0 1 1-5.8-1.6" />
              </Glyph>
              {origin}
            </span>
          ) : null}
        </div>

        {/* title */}
        <div
          style={{
            fontSize: 19,
            fontWeight: 600,
            lineHeight: 1.3,
            color: C.text,
          }}
        >
          {title}
        </div>

        {/* property chip box */}
        <div
          style={{
            alignSelf: "stretch",
            boxSizing: "border-box",
            borderRadius: 12,
            backgroundColor: C.fillCard,
            border: `1px solid rgba(255,255,255,0.06)`,
            padding: 10,
            display: "flex",
            flexWrap: "wrap",
            gap: 6,
          }}
        >
          {chip(
            <>
              <MStatusIcon status={shownStatus} size={12} />
              {shownStatusLabel}
            </>,
            "status"
          )}
          {chip(
            <>
              <MPriorityIcon
                priority={priorityLabel === "No priority" ? "none" : "high"}
                size={12}
              />
              {priorityLabel}
            </>,
            "priority"
          )}
          {labelChip
            ? chip(
                <>
                  <span
                    style={{
                      width: 8,
                      height: 8,
                      borderRadius: 999,
                      backgroundColor: labelChip.dot,
                    }}
                  />
                  {labelChip.name}
                </>,
                "label"
              )
            : null}
          {chip(<span style={{ color: C.muted }}>+</span>, "add")}
        </div>

        {/* description — near-white body text like the app */}
        <div
          style={{
            fontSize: 13,
            lineHeight: 1.6,
            color: "#d4d4d4",
          }}
        >
          {description}
        </div>

        {/* coding session status — a plain hairline row */}
        {pr ? (
          <div
            style={{
              alignSelf: "stretch",
              borderTop: `1px solid rgba(255,255,255,0.08)`,
              borderBottom: `1px solid rgba(255,255,255,0.08)`,
            }}
          >
            <div
              style={{
                height: 38,
                display: "flex",
                alignItems: "center",
                gap: 8,
              }}
            >
              <span
                style={{
                  width: 7,
                  height: 7,
                  borderRadius: 999,
                  backgroundColor: merged ? C.statusDone : C.green,
                }}
              />
              <span
                style={{
                  fontSize: 11.5,
                  fontWeight: 600,
                  color: merged ? C.statusDone : C.green,
                }}
              >
                {merged ? "Merged" : "Ready for review"}
              </span>
              <span
                style={{
                  fontSize: 11.5,
                  color: C.muted,
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }}
              >
                {`· ${pr.user} · ${pr.device}`}
              </span>
              <span
                style={{ marginLeft: "auto", color: C.dim, display: "flex" }}
              >
                <Glyph size={11} sw={2.2}>
                  <path d="m9 18 6-6-6-6" />
                </Glyph>
              </span>
            </div>
          </div>
        ) : null}

        {/* the PR — a rounded glass CARD with the state right-aligned */}
        {pr ? (
          <div
            style={{
              alignSelf: "stretch",
              height: 44,
              boxSizing: "border-box",
              display: "flex",
              alignItems: "center",
              gap: 9,
              padding: "0 13px",
              borderRadius: 12,
              backgroundColor: C.fillCard,
              border: `1px solid rgba(255,255,255,0.06)`,
            }}
          >
            <span
              style={{
                color: merged ? C.statusDone : C.green,
                display: "flex",
              }}
            >
              <Glyph size={14} sw={2}>
                <circle cx="6" cy="6" r="3" />
                <circle cx="18" cy="18" r="3" />
                <path d="M13 6h3a2 2 0 0 1 2 2v7" />
                <path d="M6 9v12" />
              </Glyph>
            </span>
            <span style={{ fontSize: 12.5, fontWeight: 600, color: C.text }}>
              {`PR #${pr.number}`}
            </span>
            <span
              style={{
                marginLeft: "auto",
                display: "flex",
                alignItems: "center",
                gap: 6,
                fontSize: 12,
                color: C.muted,
              }}
            >
              {merged ? "Merged" : "Open"}
              <Glyph size={11} sw={2.2}>
                <path d="m9 18 6-6-6-6" />
              </Glyph>
            </span>
          </div>
        ) : null}

        {/* activity timeline */}
        {activity && activity.length > 0 ? (
          <div
            style={{
              alignSelf: "stretch",
              display: "flex",
              flexDirection: "column",
              gap: 10,
            }}
          >
            <div style={{ fontSize: 13, fontWeight: 600, color: C.muted }}>
              Activity
            </div>
            {activity.map((line) => (
              <div
                key={line}
                style={{ display: "flex", alignItems: "flex-start", gap: 9 }}
              >
                <span
                  style={{
                    width: 6,
                    height: 6,
                    marginTop: 4,
                    borderRadius: 999,
                    backgroundColor: "rgba(255,255,255,0.25)",
                    flexShrink: 0,
                  }}
                />
                <span
                  style={{ fontSize: 11.5, lineHeight: 1.5, color: C.muted }}
                >
                  {line}
                </span>
              </div>
            ))}
          </div>
        ) : null}
      </div>

      {/* floating bottom bar: properties circle · "+ Comment" capsule · start circle */}
      <div
        style={{
          position: "absolute",
          left: 12,
          right: 12,
          bottom: 14,
          display: "flex",
          alignItems: "center",
          gap: 8,
        }}
      >
        <span
          style={{
            width: 44,
            height: 44,
            flexShrink: 0,
            borderRadius: 999,
            backgroundColor: "rgba(23,23,23,0.94)",
            border: `1px solid ${C.strokeCard}`,
            boxShadow: "0 10px 30px rgba(0,0,0,0.5)",
            color: C.muted,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <Glyph size={15} sw={2}>
            <path d="M21 4h-7" />
            <path d="M10 4H3" />
            <path d="M21 12h-9" />
            <path d="M8 12H3" />
            <path d="M21 20h-5" />
            <path d="M12 20H3" />
            <path d="M14 2v4" />
            <path d="M8 10v4" />
            <path d="M16 18v4" />
          </Glyph>
        </span>
        <span
          style={{
            flex: 1,
            height: 40,
            boxSizing: "border-box",
            borderRadius: 999,
            backgroundColor: "rgba(23,23,23,0.94)",
            border: `1px solid ${C.strokeCard}`,
            boxShadow: "0 10px 30px rgba(0,0,0,0.5)",
            color: C.dim,
            display: "flex",
            alignItems: "center",
            gap: 7,
            padding: "0 14px",
            fontSize: 12.5,
          }}
        >
          <Glyph size={13} sw={2.2}>
            <path d="M5 12h14" />
            <path d="M12 5v14" />
          </Glyph>
          Comment
        </span>
        <span
          style={{
            width: 44,
            height: 44,
            flexShrink: 0,
            borderRadius: 999,
            backgroundColor: "rgba(23,23,23,0.94)",
            border: `1px solid ${C.strokeCard}`,
            boxShadow: "0 10px 30px rgba(0,0,0,0.5)",
            color: C.text,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            scale: String(1 - 0.06 * pressT),
            filter: pressT > 0 ? `brightness(${1 + 0.2 * pressT})` : undefined,
          }}
        >
          {sessionLive ? (
            <span
              style={{
                width: 9,
                height: 9,
                borderRadius: 999,
                backgroundColor: C.green,
              }}
            />
          ) : (
            <Glyph size={15} sw={2}>
              <path d="M7 5.5 18.5 12 7 18.5Z" />
            </Glyph>
          )}
        </span>
      </div>
    </div>
  )
}
