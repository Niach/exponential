// closedloop/surfaces/mobileui.tsx — the REAL mobile app screens (EXP-388).
// Every phone shot in the film renders one of these, mirrored off the shipping
// iOS/Android UI (IssueListView / IssueDetailView / MobileTabBar): the grouped
// board list with glass rows and the floating icon-only glass tab bar + the
// detached compose circle, and the issue detail with its identifier pill,
// property chip box and the icon-only play circle in the floating bottom bar.
// Nothing invented: no filter-chip strips, no text labels under tab icons, no
// visible "Start coding" label on the detail screen. The mobile Review
// screen (diff cards + the × · Merge · ↗ bar) lives in reviewphone.tsx.
// All frame props are LOCAL to the segment that renders the surface.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, MONO_FONT, POP, UI_FONT } from "../../ships/theme"
import { ExpLogo } from "../../ships/rig"
import { ClaudeMark } from "./agentmarks"

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
export type MobileStatus = "backlog" | "in_progress" | "in_review" | "done"

export const statusColor = (s: MobileStatus): string =>
  s === "backlog"
    ? C.statusBacklog
    : s === "in_progress"
      ? C.statusInProgress
      : s === "in_review"
        ? C.statusInReview
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
    ) : status === "in_progress" ? (
      <Glyph size={size} sw={2}>
        <circle cx="12" cy="12" r="10" />
        <path
          d="M12 12 L12 6 A6 6 0 0 1 12 18 Z"
          fill="currentColor"
          stroke="none"
        />
      </Glyph>
    ) : status === "in_review" ? (
      // In Review: progress-3-4 — three quarters filled.
      <Glyph size={size} sw={2}>
        <circle cx="12" cy="12" r="10" />
        <path
          d="M12 12 L12 6 A6 6 0 1 1 6 12 Z"
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

// ── iOS glass tokens (apps/ios/ExpUI DesignTokens.generated.swift) ──────────
// The phone speaks its OWN token values, not the desktop ladder in ships/theme:
// white 0.0588 row/card fills, 0.0392 section bands, the 0.5pt hairline and the
// OPAQUE card composite (`GlassTokens.opaqueCardFill` = fillCard over #171717)
// every floating bar wears.
export const IOS = {
  fillSection: "rgba(255,255,255,0.0392)",
  fillRow: "rgba(255,255,255,0.0588)",
  fillCard: "rgba(255,255,255,0.0588)",
  fillActive: "rgba(255,255,255,0.1098)",
  strokeRow: "rgba(255,255,255,0.0588)",
  strokeSection: "rgba(255,255,255,0.0784)",
  strokeCard: "rgba(255,255,255,0.1020)",
  strokeStrong: "rgba(255,255,255,0.1216)",
  strokeActive: "rgba(255,255,255,0.14)",
  opaqueCard: "#252525", // fillCard (5.88% white) over Palette.card #171717
  card: "#171717",
  backgroundBottom: "#111114",
  // TextOpacity ladder (Typography.Emphasis)
  secondary: "rgba(255,255,255,0.7)",
  tertiary: "rgba(255,255,255,0.5)",
  quaternary: "rgba(255,255,255,0.3)",
  band: "rgba(255,255,255,0.85)", // GlassSectionBand label rung
  primary: "#e5e5e5", // Palette.primary
  primaryFg: "#171717",
  muted: "#a1a1a1", // Palette.mutedForeground
  green: "#22c55e", // Semantic.green
  yellow: "#facc15", // Semantic.yellow
  blue: "#3b82f6", // Semantic.blue
  red: "#ef4444", // Semantic.red
  destructive: "#ff6467",
} as const

// iOS points → phone px: the chassis screen is 312 px for the 414 pt device.
export const PT_SCALE = 312 / 414
export const pt = (v: number): number => Math.round(v * PT_SCALE * 100) / 100
// The hairline (0.5 pt) never drops below one device pixel in the film.
export const HAIRLINE = 0.75

// UserAvatar's fallback chip: the hue picked off the member key (Avatar.hues),
// the initials in it at 0.42 of the diameter over the hue at 20 %.
const AVATAR_HUES = [
  "#f87171",
  "#fb923c",
  "#facc15",
  "#4ade80",
  "#2dd4bf",
  "#60a5fa",
  "#a78bfa",
  "#f472b6",
] as const
export const avatarHue = (key: string): string => {
  let h = 0
  for (const ch of key) h = (h * 31 + ch.charCodeAt(0)) >>> 0
  return AVATAR_HUES[h % AVATAR_HUES.length]
}
export const MAvatar: React.FC<{ initials: string; size: number }> = ({
  initials,
  size,
}) => {
  const hue = avatarHue(initials)
  return (
    <span
      style={{
        width: size,
        height: size,
        flexShrink: 0,
        borderRadius: 999,
        backgroundColor: `${hue}33`,
        color: hue,
        fontSize: size * 0.42,
        fontWeight: 500,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      {initials}
    </span>
  )
}

// ── The floating tab bar (MobileTabBar.swift) ────────────────────────────────
// NEW-7/EXP-1291: four tabs in ONE pill — Issues (`list`) · Inbox (`inbox`,
// the primary unread dot) · Devices (`monitor`) · Threads (`list-tree`, green
// while a thread runs, amber while one needs you) — 44pt slots, 2pt apart,
// 3pt pill padding, opaqueCardFill under the strokeStrong hairline; the active
// tab wears the fillActive CIRCLE. Right: the SPLIT launcher capsule (EXP-973/
// 827) — chat (`message-circle`) | New issue (`square-pen`), two 52pt arms
// split by a 28pt hairline. Bar inset 12pt, 4pt off the bottom safe area.
// Legacy dot ids the segments still pass ("actions", "reviews") have no tab
// any more and are dropped.
const TAB_ICONS: { id: string; node: (s: number) => React.ReactNode }[] = [
  {
    id: "issues",
    node: (s) => (
      <Glyph size={s} sw={2}>
        <path d="M3 12h.01" />
        <path d="M3 18h.01" />
        <path d="M3 6h.01" />
        <path d="M8 12h13" />
        <path d="M8 18h13" />
        <path d="M8 6h13" />
      </Glyph>
    ),
  },
  {
    id: "mywork",
    node: (s) => (
      <Glyph size={s} sw={2}>
        <polyline points="22 12 16 12 14 15 10 15 8 12 2 12" />
        <path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />
      </Glyph>
    ),
  },
  {
    id: "devices",
    node: (s) => (
      <Glyph size={s} sw={2}>
        <rect x="2" y="3" width="20" height="14" rx="2" />
        <path d="M8 21h8" />
        <path d="M12 17v4" />
      </Glyph>
    ),
  },
  {
    id: "threads",
    node: (s) => (
      <Glyph size={s} sw={2}>
        <path d="M21 12h-8" />
        <path d="M21 6H8" />
        <path d="M21 18h-8" />
        <path d="M3 6v4c0 1.1.9 2 2 2h3" />
        <path d="M3 10v6c0 1.1.9 2 2 2h3" />
      </Glyph>
    ),
  },
]

export const MobileTabBar: React.FC<{
  active?: string
  dots?: readonly string[] // tab ids carrying a dot
  /** Threads dot tone: green = a thread runs, amber = one needs you. */
  threadsDot?: "green" | "amber"
}> = ({ active = "issues", dots = [], threadsDot = "green" }) => {
  const slot = pt(44)
  const glyph = pt(20)
  const dotColor = (id: string) =>
    id === "threads"
      ? threadsDot === "amber"
        ? IOS.yellow
        : IOS.green
      : IOS.primary
  return (
    <div
      style={{
        position: "absolute",
        left: pt(12),
        right: pt(12),
        bottom: pt(4) + pt(34),
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: pt(2),
          padding: pt(3),
          borderRadius: 999,
          backgroundColor: IOS.opaqueCard,
          boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeStrong}`,
        }}
      >
        {TAB_ICONS.map(({ id, node }) => (
          <span
            key={id}
            style={{
              position: "relative",
              width: slot,
              height: slot,
              borderRadius: 999,
              backgroundColor: id === active ? IOS.fillActive : "transparent",
              color: id === active ? "#ffffff" : IOS.secondary,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            {node(glyph)}
            {dots.includes(id) ? (
              <span
                style={{
                  position: "absolute",
                  top: pt(8) - pt(4),
                  right: pt(8) - pt(4),
                  width: pt(8),
                  height: pt(8),
                  borderRadius: 999,
                  backgroundColor: dotColor(id),
                }}
              />
            ) : null}
          </span>
        ))}
      </div>
      {/* the split launcher: chat | New issue */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          borderRadius: 999,
          backgroundColor: IOS.opaqueCard,
          boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeStrong}`,
          color: "#ffffff",
        }}
      >
        <span
          style={{
            width: pt(52),
            height: pt(52),
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <Glyph size={glyph} sw={2.2}>
            <path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z" />
          </Glyph>
        </span>
        <span
          style={{
            width: HAIRLINE,
            height: pt(28),
            backgroundColor: IOS.strokeStrong,
          }}
        />
        <span
          style={{
            width: pt(52),
            height: pt(52),
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <Glyph size={glyph} sw={2.2}>
            <path d="M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
            <path d="M18.375 2.625a1 1 0 0 1 3 3l-9.013 9.014a2 2 0 0 1-.853.505l-2.873.84a.5.5 0 0 1-.62-.62l.84-2.873a2 2 0 0 1 .506-.852z" />
          </Glyph>
        </span>
      </div>
    </div>
  )
}

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
  due?: string
}

// Contract displayOrder (issueStatusDefaults) — the same order the desktop and
// web lists group by: backlog → unstarted → started → completed.
const SECTION_ORDER: { status: MobileStatus; name: string }[] = [
  { status: "backlog", name: "Backlog" },
  { status: "in_progress", name: "In Progress" },
  { status: "in_review", name: "In Review" },
  { status: "done", name: "Done" },
]

// IssueListView metrics (pt): the first status band 115pt down (under the
// 44pt nav bar), bands 28pt tall + 4pt under them, rows ~39.5pt on a 42.5pt
// pitch (listRowInsets 1.5 top/bottom), 6pt above every following band.
const LIST_TOP = pt(115)
const HEADER_H = pt(28)
const HEADER_GAP = pt(5.5)
const ROW_H = pt(39.5)
const ROW_GAP = pt(3)
const GROUP_GAP = pt(4.5)

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
    y += HEADER_H + HEADER_GAP
    for (const row of inSection) {
      els.push({ key: row.id, y })
      y += ROW_H + ROW_GAP
    }
    y += GROUP_GAP
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
  const NAV_Y = pt(79) // the nav bar's centre line (status bar + 44pt bar)

  return (
    <div style={{ position: "absolute", inset: 0, fontFamily: UI_FONT }}>
      {/* nav bar (IssuesHomeView + shots/board/ios.webp): the board
          switcher is the PRINCIPAL item — a CENTERED md GlassPill: the board
          glyph in the board colour, the name, the chevrons-up-down expander —
          and search · settings share ONE trailing glass capsule (EXP-686
          moved Search out of the tab bar; EXP-862 removed filtering). */}
      <div
        style={{
          position: "absolute",
          top: NAV_Y - pt(16),
          left: 0,
          right: 0,
          display: "flex",
          justifyContent: "center",
        }}
      >
        <div
          style={{
            height: pt(32),
            boxSizing: "border-box",
            display: "flex",
            alignItems: "center",
            gap: pt(6),
            padding: `0 ${pt(12)}px`,
            borderRadius: 999,
            backgroundColor: IOS.fillCard,
            boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeCard}`,
          }}
        >
          <span style={{ color: "#818cf8", display: "flex" }}>
            <Glyph size={pt(16)} sw={2}>
              <path d="m16 18 6-6-6-6" />
              <path d="m8 6-6 6 6 6" />
            </Glyph>
          </span>
          <span style={{ fontSize: pt(15), fontWeight: 500, color: "#ffffff" }}>
            {boardName}
          </span>
          <span style={{ color: IOS.secondary, display: "flex" }}>
            <Glyph size={pt(16)} sw={2}>
              <path d="m7 15 5 5 5-5" />
              <path d="m7 9 5-5 5 5" />
            </Glyph>
          </span>
        </div>
      </div>
      <div
        style={{
          position: "absolute",
          top: NAV_Y - pt(22),
          right: pt(16),
          height: pt(44),
          boxSizing: "border-box",
          display: "flex",
          alignItems: "center",
          gap: pt(12),
          padding: `0 ${pt(14)}px`,
          borderRadius: 999,
          backgroundColor: IOS.opaqueCard,
          boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeStrong}`,
          color: IOS.secondary,
        }}
      >
        <span style={{ width: pt(32), display: "flex", justifyContent: "center" }}>
          <Glyph size={pt(17)} sw={2}>
            <circle cx="11" cy="11" r="8" />
            <path d="m21 21-4.3-4.3" />
          </Glyph>
        </span>
        <span style={{ width: pt(32), display: "flex", justifyContent: "center" }}>
          <Glyph size={pt(17)} sw={2}>
            <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
            <circle cx="12" cy="12" r="3" />
          </Glyph>
        </span>
      </div>

      {/* grouped list: GlassSectionBand per status (the ONE band that keeps
          a count) over glass rows */}
      {SECTION_ORDER.map((section) => {
        const pos = posOf(`h:${section.status}`)
        if (pos.o <= 0.01) return null
        const count = live.counts[section.status] ?? 0
        return (
          <div
            key={section.status}
            style={{
              position: "absolute",
              left: pt(16),
              right: pt(16),
              top: pos.y,
              height: HEADER_H,
              boxSizing: "border-box",
              display: "flex",
              alignItems: "center",
              gap: pt(6),
              padding: `0 ${pt(12)}px`,
              borderRadius: pt(10),
              backgroundColor: IOS.fillSection,
              opacity: pos.o,
            }}
          >
            <span
              style={{
                width: pt(12),
                color: IOS.tertiary,
                display: "flex",
                justifyContent: "center",
              }}
            >
              <Glyph size={pt(11)} sw={2.2}>
                <path d="m6 9 6 6 6-6" />
              </Glyph>
            </span>
            <MStatusIcon status={section.status} size={pt(13)} />
            <span
              style={{
                fontSize: pt(15),
                fontWeight: 500,
                color: IOS.band,
              }}
            >
              {section.name}
            </span>
            <span style={{ fontSize: pt(12), color: IOS.tertiary }}>
              {count}
            </span>
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
              left: pt(16),
              right: pt(16),
              top: pos.y,
              height: ROW_H,
              boxSizing: "border-box",
              display: "flex",
              alignItems: "center",
              gap: pt(10),
              padding: `0 ${pt(12)}px`,
              borderRadius: pt(10),
              backgroundColor: IOS.fillRow,
              boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeRow}`,
              opacity: pos.o,
            }}
          >
            <span style={{ width: pt(16), display: "flex", justifyContent: "center" }}>
              <MPriorityIcon priority={data.priority} size={pt(13)} />
            </span>
            <span
              style={{
                width: pt(52),
                fontFamily: MONO_FONT,
                fontSize: pt(12),
                color: IOS.tertiary,
                flexShrink: 0,
                whiteSpace: "nowrap",
              }}
            >
              {data.id}
            </span>
            <span style={{ width: pt(16), display: "flex", justifyContent: "center" }}>
              <MStatusIcon status={status} size={pt(16)} />
            </span>
            <span
              style={{
                fontSize: pt(15),
                color: "#ffffff",
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
                flex: 1,
                minWidth: 0,
              }}
            >
              {data.title}
            </span>
            {/* trailing meta: labels · due · assignee · the chevron INSIDE
                the card (EXP-698 r7) — one fixed-size group */}
            <span
              style={{
                display: "flex",
                alignItems: "center",
                gap: pt(10),
                flexShrink: 0,
              }}
            >
              {data.labelDot ? (
                <span
                  style={{
                    width: pt(8),
                    height: pt(8),
                    borderRadius: 999,
                    backgroundColor: data.labelDot,
                  }}
                />
              ) : null}
              {data.due ? (
                <span
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: pt(3),
                    fontSize: pt(12),
                    color: IOS.secondary,
                    whiteSpace: "nowrap",
                  }}
                >
                  <Glyph size={pt(11)} sw={2}>
                    <path d="M8 2v4" />
                    <path d="M16 2v4" />
                    <rect x="3" y="4" width="18" height="18" rx="2" />
                    <path d="M3 10h18" />
                    <path d="M8 14h.01" />
                    <path d="M12 14h.01" />
                    <path d="M16 14h.01" />
                    <path d="M8 18h.01" />
                    <path d="M12 18h.01" />
                    <path d="M16 18h.01" />
                  </Glyph>
                  {data.due}
                </span>
              ) : null}
              {data.assignee ? (
                <MAvatar initials={data.assignee} size={pt(22)} />
              ) : null}
              <span style={{ color: IOS.tertiary, display: "flex" }}>
                <Glyph size={pt(16)} sw={2}>
                  <path d="m9 18 6-6-6-6" />
                </Glyph>
              </span>
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
/** One activity row: a status change wears its status glyph, anything else a
 *  plain rail dot (CommentThreadView's gutter markers). */
export type ActivityRow = { text: string; status?: MobileStatus }

export type IssueScreenProps = {
  frame: number
  identifier: string
  title: string
  origin?: string // "Feedback widget" provenance chip above the title
  status: MobileStatus
  statusLabel: string
  priorityLabel: string
  /** Assignee chip (avatar + display name) — hidden on solo teams, like the app. */
  assignee?: { name: string; initials: string }
  /** Due-date chip ("Jul 19") — rendered only when the issue carries one. */
  due?: string
  labelChip?: { name: string; dot: string }
  description: string
  /** Coding/PR status rows (hairline rows, not a box); ready → merged at mergedAt. */
  pr?: {
    number: number
    device: string
    user: string
    mergedAt?: number
    title?: string
  }
  /** Activity timeline rows on the gutter rail (e.g. "… created the issue · 1 hr ago"). */
  activity?: readonly ActivityRow[]
  /** Press flash on the play circle (the icon-only start button). */
  playPressAt?: number
  /** Legacy (pre EXP-1295 the Start circle turned into a live dot); the
   *  Start circle is now ALWAYS the bar's trailing control — ignored. */
  sessionLive?: boolean
  /** The board chip (last in the chip box). */
  boardName?: string
}

// ── Shared iOS chrome atoms ─────────────────────────────────────────────────
/** A nav-bar glass circle (back / `…`): 41pt, opaque card fill, hairline. */
export const NavCircle: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => (
  <span
    style={{
      width: pt(41),
      height: pt(41),
      flexShrink: 0,
      borderRadius: 999,
      backgroundColor: IOS.opaqueCard,
      boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeStrong}`,
      color: "#ffffff",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
    }}
  >
    {children}
  </span>
)
export const BackGlyph: React.FC = () => (
  <Glyph size={pt(20)} sw={2.4}>
    <path d="m15 18-6-6 6-6" />
  </Glyph>
)
export const MoreGlyph: React.FC = () => (
  <Glyph size={pt(18)} sw={2.6}>
    <circle cx="5" cy="12" r="0.8" fill="currentColor" />
    <circle cx="12" cy="12" r="0.8" fill="currentColor" />
    <circle cx="19" cy="12" r="0.8" fill="currentColor" />
  </Glyph>
)
/** FloatingBottomBar's 52pt glass circle. */
export const BarCircle: React.FC<{
  children: React.ReactNode
  style?: React.CSSProperties
}> = ({ children, style }) => (
  <span
    style={{
      width: pt(52),
      height: pt(52),
      flexShrink: 0,
      borderRadius: 999,
      backgroundColor: IOS.opaqueCard,
      boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeStrong}`,
      color: "#ffffff",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      ...style,
    }}
  >
    {children}
  </span>
)
/** The thread chip's lead (IssueThreadsStrip): Claude's mark, amber-dotted
 *  when the thread needs you. */
export const ThreadMark: React.FC<{ size: number; dot?: boolean }> = ({
  size,
  dot,
}) => (
  <span style={{ position: "relative", display: "flex" }}>
    <ClaudeMark size={size} />
    {dot ? (
      <span
        style={{
          position: "absolute",
          top: -size * 0.12,
          right: -size * 0.12,
          width: size * 0.42,
          height: size * 0.42,
          borderRadius: 999,
          backgroundColor: IOS.yellow,
        }}
      />
    ) : null}
  </span>
)

export const IssueScreen: React.FC<IssueScreenProps> = ({
  frame,
  identifier,
  title,
  origin,
  status,
  statusLabel,
  priorityLabel,
  assignee,
  due,
  labelChip,
  description,
  pr,
  activity,
  playPressAt,
  boardName = "Acme Shop",
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
  // issue-lifecycle.json (NEW-1): `pr_opened` moves a linked issue from
  // Backlog/In Progress to In Review and `pr_merged` to Done — so with a PR
  // row on the page the chip can only read In Review (open) or Done.
  const shownStatus: MobileStatus = merged
    ? "done"
    : pr && (status === "backlog" || status === "in_progress")
      ? "in_review"
      : status
  const shownStatusLabel = merged
    ? "Done"
    : shownStatus === "in_review" && status !== "in_review"
      ? "In Review"
      : statusLabel
  // EXP-1295: once a session holds the issue, the page names its THREAD.
  const threadTitle = pr ? `#${identifier} ${title}` : undefined

  // GlassPill .sm in the chip box: 22pt tall (as drawn), caption-medium.
  const chip = (children: React.ReactNode, key?: string) => (
    <span
      key={key}
      style={{
        height: pt(22),
        boxSizing: "border-box",
        display: "inline-flex",
        alignItems: "center",
        gap: pt(4),
        padding: `0 ${pt(8)}px`,
        borderRadius: 999,
        backgroundColor: IOS.fillCard,
        boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeCard}`,
        fontSize: pt(12),
        fontWeight: 500,
        color: "rgba(255,255,255,0.85)",
        whiteSpace: "nowrap",
      }}
    >
      {children}
    </span>
  )

  const NAV_Y = pt(79)

  return (
    <div style={{ position: "absolute", inset: 0, fontFamily: UI_FONT }}>
      {/* nav bar (IssueScreen.swift): back circle · the IDENTIFIER · the
          `…` circle (Share, Move to board, Delete) */}
      <div
        style={{
          position: "absolute",
          top: NAV_Y - pt(20.5),
          left: pt(19),
          right: pt(19),
          height: pt(41),
          display: "flex",
          alignItems: "center",
        }}
      >
        <NavCircle>
          <BackGlyph />
        </NavCircle>
        <span
          style={{
            position: "absolute",
            left: 0,
            right: 0,
            textAlign: "center",
            fontSize: pt(17),
            fontWeight: 600,
            color: "#ffffff",
          }}
        >
          {identifier}
        </span>
        <span style={{ marginLeft: "auto" }}>
          <NavCircle>
            <MoreGlyph />
          </NavCircle>
        </span>
      </div>

      {/* the issue page (IssueFaceView): 20pt padding, 20pt between blocks —
          origin pill → title → chip box → Threads row → description →
          sub-issues → PR rows → activity */}
      <div
        style={{
          position: "absolute",
          top: pt(108) + pt(20),
          left: pt(20),
          right: pt(20),
          display: "flex",
          flexDirection: "column",
          gap: pt(20),
          alignItems: "flex-start",
        }}
      >
        {origin ? (
          <span
            style={{
              height: pt(24),
              boxSizing: "border-box",
              display: "inline-flex",
              alignItems: "center",
              gap: pt(4),
              padding: `0 ${pt(8)}px`,
              borderRadius: 999,
              backgroundColor: IOS.fillCard,
              boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeCard}`,
              fontSize: pt(12),
              fontWeight: 500,
              color: "rgba(255,255,255,0.85)",
            }}
          >
            <span style={{ color: IOS.secondary, display: "flex" }}>
              {/* ui-widget = megaphone */}
              <Glyph size={pt(12)} sw={2}>
                <path d="m3 11 18-5v12L3 14v-3z" />
                <path d="M11.6 16.8a3 3 0 1 1-5.8-1.6" />
              </Glyph>
            </span>
            {origin}
          </span>
        ) : null}

        {/* title — title2 semibold */}
        <div
          style={{
            fontSize: pt(22),
            fontWeight: 600,
            lineHeight: 1.27,
            color: "#ffffff",
            marginTop: origin ? -pt(8) : 0,
          }}
        >
          {title}
        </div>

        {/* property chip box: status · priority · assignee · labels · + ·
            due · board (P46 order) in ONE glass card */}
        <div
          style={{
            alignSelf: "stretch",
            boxSizing: "border-box",
            borderRadius: pt(16),
            backgroundColor: IOS.fillCard,
            boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeCard}`,
            padding: pt(10),
            display: "flex",
            flexWrap: "wrap",
            gap: pt(6),
          }}
        >
          {chip(
            <>
              <MStatusIcon status={shownStatus} size={pt(12)} />
              {shownStatusLabel}
            </>,
            "status"
          )}
          {chip(
            <>
              <MPriorityIcon
                priority={priorityLabel === "No priority" ? "none" : "high"}
                size={pt(12)}
              />
              {priorityLabel}
            </>,
            "priority"
          )}
          {assignee
            ? chip(
                <>
                  <MAvatar initials={assignee.initials} size={pt(16)} />
                  {assignee.name}
                </>,
                "assignee"
              )
            : null}
          {labelChip
            ? chip(
                <>
                  <span
                    style={{
                      width: pt(6),
                      height: pt(6),
                      borderRadius: 999,
                      backgroundColor: labelChip.dot,
                    }}
                  />
                  {labelChip.name}
                </>,
                "label"
              )
            : null}
          {chip(
            <span style={{ color: IOS.secondary, display: "flex" }}>
              <Glyph size={pt(12)} sw={2}>
                <path d="M5 12h14" />
                <path d="M12 5v14" />
              </Glyph>
            </span>,
            "add"
          )}
          {due
            ? chip(
                <>
                  <span style={{ color: IOS.secondary, display: "flex" }}>
                    <Glyph size={pt(12)} sw={2}>
                      <path d="M8 2v4" />
                      <path d="M16 2v4" />
                      <rect x="3" y="4" width="18" height="18" rx="2" />
                      <path d="M3 10h18" />
                    </Glyph>
                  </span>
                  {due}
                </>,
                "due"
              )
            : null}
          {chip(
            <>
              <span style={{ color: "#818cf8", display: "flex" }}>
                <Glyph size={pt(12)} sw={2}>
                  <path d="m16 18 6-6-6-6" />
                  <path d="m8 6-6 6 6 6" />
                </Glyph>
              </span>
              {boardName}
            </>,
            "board"
          )}
        </div>

        {/* EXP-1295: the Threads row — one chip per thread holding it */}
        {threadTitle ? (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: pt(10),
              maxWidth: "100%",
            }}
          >
            <span style={{ fontSize: pt(13), color: IOS.tertiary }}>Threads</span>
            <span
              style={{
                display: "flex",
                alignItems: "center",
                gap: pt(6),
                minWidth: 0,
                padding: `${pt(4)}px ${pt(8)}px`,
                borderRadius: pt(5),
                backgroundColor: "rgba(255,255,255,0.08)",
                boxShadow: `inset 0 0 0 ${HAIRLINE}px rgba(255,255,255,0.16)`,
              }}
            >
              <ThreadMark size={pt(12)} />
              <span
                style={{
                  fontSize: pt(13),
                  color: "#ffffff",
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  maxWidth: pt(220),
                }}
              >
                {threadTitle}
              </span>
            </span>
          </div>
        ) : null}

        {/* description — the markdown editor (body text, 200pt floor) */}
        <div
          style={{
            alignSelf: "stretch",
            minHeight: pt(threadTitle ? 120 : 200),
            fontSize: pt(17),
            lineHeight: 1.3,
            color: "#ffffff",
            paddingLeft: pt(7),
          }}
        >
          {description}
        </div>

        {/* EXP-1097: the lone "Add sub-issues" row */}
        <div
          style={{
            alignSelf: "stretch",
            height: pt(40),
            boxSizing: "border-box",
            display: "flex",
            alignItems: "center",
            gap: pt(8),
            padding: `0 ${pt(12)}px`,
            borderRadius: pt(10),
            backgroundColor: IOS.fillRow,
            fontSize: pt(15),
            color: IOS.tertiary,
          }}
        >
          <Glyph size={pt(14)} sw={2}>
            <path d="M5 12h14" />
            <path d="M12 5v14" />
          </Glyph>
          Add sub-issues
        </div>

        {/* PR rows (AgentPrCard): the PR glyph in the state tint · "PR #n"
            · title · state · external link, on a glass row */}
        {pr ? (
          <div
            style={{
              alignSelf: "stretch",
              boxSizing: "border-box",
              display: "flex",
              alignItems: "center",
              gap: pt(8),
              padding: `${pt(10)}px ${pt(12)}px`,
              borderRadius: pt(10),
              backgroundColor: IOS.fillRow,
              boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeRow}`,
            }}
          >
            <span
              style={{ color: merged ? IOS.blue : IOS.green, display: "flex" }}
            >
              <Glyph size={pt(13)} sw={2.4}>
                <circle cx="18" cy="18" r="3" />
                <circle cx="6" cy="6" r="3" />
                <path d="M13 6h3a2 2 0 0 1 2 2v7" />
                <path d="M6 9v12" />
              </Glyph>
            </span>
            <span
              style={{
                fontSize: pt(12),
                fontWeight: 500,
                color: "#ffffff",
                flexShrink: 0,
              }}
            >
              {`PR #${pr.number}`}
            </span>
            <span
              style={{
                fontSize: pt(12),
                color: IOS.secondary,
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
                flex: 1,
                minWidth: 0,
              }}
            >
              {pr.title ?? title}
            </span>
            <span style={{ fontSize: pt(11), color: IOS.secondary, flexShrink: 0 }}>
              {merged ? "Merged" : "Open"}
            </span>
            <span style={{ color: IOS.tertiary, display: "flex" }}>
              <Glyph size={pt(13)} sw={2}>
                <path d="M15 3h6v6" />
                <path d="M10 14 21 3" />
                <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
              </Glyph>
            </span>
          </div>
        ) : null}

        {/* activity timeline (CommentThreadView) */}
        {activity && activity.length > 0 ? (
          <>
            <div
              style={{
                alignSelf: "stretch",
                height: HAIRLINE,
                marginLeft: -pt(20),
                marginRight: -pt(20),
                backgroundColor: IOS.strokeCard,
              }}
            />
            <div style={{ alignSelf: "stretch", marginTop: -pt(4) }}>
              <div
                style={{
                  fontSize: pt(17),
                  fontWeight: 600,
                  color: "#ffffff",
                  marginBottom: pt(10),
                }}
              >
                Activity
              </div>
              <div>
                {activity.map((row, i) => (
                  <div
                    key={row.text}
                    style={{
                      position: "relative",
                      display: "flex",
                      alignItems: "flex-start",
                      gap: pt(12),
                      paddingBottom: pt(8),
                    }}
                  >
                    {i < activity.length - 1 ? (
                      <span
                        style={{
                          position: "absolute",
                          left: pt(7.5),
                          top: pt(12),
                          bottom: -pt(12),
                          width: HAIRLINE,
                          backgroundColor: IOS.strokeStrong,
                        }}
                      />
                    ) : null}
                    <span
                      style={{
                        width: pt(16),
                        height: pt(20),
                        flexShrink: 0,
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "center",
                      }}
                    >
                      {row.status ? (
                        <MStatusIcon status={row.status} size={pt(13)} />
                      ) : (
                        <span
                          style={{
                            width: pt(6),
                            height: pt(6),
                            borderRadius: 999,
                            backgroundColor: IOS.quaternary,
                          }}
                        />
                      )}
                    </span>
                    <span
                      style={{
                        fontSize: pt(14),
                        lineHeight: `${pt(20)}px`,
                        color: IOS.tertiary,
                      }}
                    >
                      {row.text}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          </>
        ) : null}
      </div>

      {/* the floating bottom bar (IssueDetailBottomBar on FloatingBottomBar):
          properties circle · "+ Comment" capsule · the Start circle */}
      <div
        style={{
          position: "absolute",
          left: pt(20),
          right: pt(20),
          bottom: pt(4) + pt(34),
          display: "flex",
          alignItems: "center",
          gap: pt(10),
        }}
      >
        <BarCircle>
          <Glyph size={pt(20)} sw={2}>
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
        </BarCircle>
        <span
          style={{
            flex: 1,
            height: pt(52),
            boxSizing: "border-box",
            borderRadius: 999,
            backgroundColor: IOS.opaqueCard,
            boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeStrong}`,
            color: IOS.tertiary,
            display: "flex",
            alignItems: "center",
            gap: pt(8),
            padding: `0 ${pt(18)}px`,
            fontSize: pt(17),
          }}
        >
          <Glyph size={pt(17)} sw={2}>
            <path d="M5 12h14" />
            <path d="M12 5v14" />
          </Glyph>
          Comment
        </span>
        <BarCircle
          style={{
            scale: String(1 - 0.06 * pressT),
            filter: pressT > 0 ? `brightness(${1 + 0.4 * pressT})` : undefined,
          }}
        >
          <Glyph size={pt(20)} sw={2}>
            <path d="M6 4.5v15a1 1 0 0 0 1.5.86l12.4-7.5a1 1 0 0 0 0-1.72L7.5 3.64A1 1 0 0 0 6 4.5z" />
          </Glyph>
        </BarCircle>
      </div>
    </div>
  )
}

// ── DiffFileCard (Issue/DiffFileCard.swift + DiffRendering.swift) ────────────
// The ONE per-file diff unit ×4 (EXP-895/916): a header band on the SECTION
// fill — status letter · path (directory dimmed, the file name bright) ·
// +n −n · chevron — over the unified patch in caption2 monospace (11pt, rows
// lineHeight + 2): two right-aligned line-number gutters (4 digits + 8pt), the
// sign column, the hunk header on the hunk wash, add/del rows on the 10 %
// green/red washes with their own ink.
export const DIFF_IOS = {
  addFg: "#34d399",
  addBg: "rgba(16,185,129,0.102)",
  delFg: "#fb7185",
  delBg: "rgba(244,63,94,0.102)",
  hunkFg: "#a1a1a1",
  hunkBg: "rgba(255,255,255,0.0392)",
  gutterFg: "rgba(161,161,161,0.502)",
} as const

export type MDiffRow = {
  t: "ctx" | "del" | "add" | "hunk"
  text: string
  old?: number
  new?: number
}

const middleTruncate = (path: string, max: number): string => {
  if (path.length <= max) return path
  const keep = Math.floor((max - 1) / 2)
  return `${path.slice(0, keep)}…${path.slice(path.length - (max - 1 - keep))}`
}

export const DiffFileCard: React.FC<{
  status: string
  path: string
  add: number
  del: number
  rows?: readonly MDiffRow[]
  open?: boolean
  /** A transcript card sits one point below the page's own diff. */
  compact?: boolean
  /** Per-row reveal opacity (index → 0…1); undefined = all painted. */
  rowOpacity?: (i: number) => number
  /** Stand-alone list item: the glassRow card around it. */
  carded?: boolean
  /** "N unchanged lines" fold above the first hunk. */
  unchanged?: number
  maxPathChars?: number
}> = ({
  status,
  path,
  add,
  del,
  rows = [],
  open = true,
  compact = false,
  rowOpacity,
  carded = true,
  unchanged,
  maxPathChars = 34,
}) => {
  const font = pt(compact ? 10 : 11)
  const rowH = pt(compact ? 13 : 15)
  const adv = font * 0.6
  const gutter = adv * 4 + pt(8)
  const sign = adv * 1.5
  const shown = middleTruncate(path, maxPathChars)
  const cut = shown.lastIndexOf("/") + 1
  return (
    <div
      style={{
        boxSizing: "border-box",
        borderRadius: pt(10),
        overflow: "hidden",
        backgroundColor: carded ? IOS.fillRow : "transparent",
        boxShadow: carded ? `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeRow}` : undefined,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: pt(8),
          padding: `${pt(compact ? 7 : 10)}px ${pt(12)}px`,
          backgroundColor: IOS.fillSection,
          fontFamily: MONO_FONT,
          fontSize: pt(compact ? 11 : 12),
        }}
      >
        <span style={{ color: IOS.tertiary }}>{status}</span>
        <span
          style={{
            flex: 1,
            minWidth: 0,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          <span style={{ color: IOS.tertiary }}>{shown.slice(0, cut)}</span>
          <span style={{ color: "#ffffff" }}>{shown.slice(cut)}</span>
        </span>
        <span style={{ fontFamily: UI_FONT, fontSize: pt(compact ? 11 : 12), color: DIFF_IOS.addFg }}>
          {`+${add}`}
        </span>
        <span style={{ fontFamily: UI_FONT, fontSize: pt(compact ? 11 : 12), color: DIFF_IOS.delFg, marginLeft: -pt(4) }}>
          {`−${del}`}
        </span>
        <span style={{ color: IOS.tertiary, display: "flex" }}>
          <Glyph size={pt(12)} sw={2.2}>
            <path d={open ? "m18 15-6-6-6 6" : "m6 9 6 6 6-6"} />
          </Glyph>
        </span>
      </div>
      {open && rows.length > 0 ? (
        <div style={{ paddingBottom: pt(6) }}>
          {unchanged !== undefined ? (
            <div
              style={{
                height: rowH,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                fontFamily: MONO_FONT,
                fontSize: font,
                color: DIFF_IOS.gutterFg,
                backgroundColor: "rgba(255,255,255,0.0235)",
              }}
            >
              {`${unchanged} unchanged lines`}
            </div>
          ) : null}
          {rows.map((row, i) => {
            const o = rowOpacity ? rowOpacity(i) : 1
            if (row.t === "hunk") {
              const head = row.text.match(/^@@[^@]*@@/)?.[0] ?? row.text
              return (
                <div
                  key={`h-${i}`}
                  style={{
                    height: rowH,
                    display: "flex",
                    alignItems: "center",
                    padding: `0 ${pt(8)}px`,
                    fontFamily: MONO_FONT,
                    fontSize: font,
                    color: DIFF_IOS.hunkFg,
                    backgroundColor: DIFF_IOS.hunkBg,
                    whiteSpace: "pre",
                    overflow: "hidden",
                    opacity: o,
                  }}
                >
                  {`${head}${row.text.slice(head.length)}`}
                </div>
              )
            }
            const fg =
              row.t === "add"
                ? DIFF_IOS.addFg
                : row.t === "del"
                  ? DIFF_IOS.delFg
                  : IOS.secondary
            const bg =
              row.t === "add"
                ? DIFF_IOS.addBg
                : row.t === "del"
                  ? DIFF_IOS.delBg
                  : "transparent"
            return (
              <div
                key={`${row.t}-${i}`}
                style={{
                  height: rowH,
                  display: "flex",
                  alignItems: "center",
                  fontFamily: MONO_FONT,
                  fontSize: font,
                  backgroundColor: bg,
                  whiteSpace: "pre",
                  overflow: "hidden",
                  opacity: o,
                }}
              >
                <span
                  style={{
                    width: gutter - pt(4),
                    paddingRight: pt(4),
                    textAlign: "right",
                    flexShrink: 0,
                    color: DIFF_IOS.gutterFg,
                  }}
                >
                  {row.old ?? ""}
                </span>
                <span
                  style={{
                    width: gutter - pt(4),
                    paddingRight: pt(4),
                    textAlign: "right",
                    flexShrink: 0,
                    color: DIFF_IOS.gutterFg,
                  }}
                >
                  {row.new ?? ""}
                </span>
                <span
                  style={{
                    width: sign,
                    flexShrink: 0,
                    textAlign: "center",
                    color: fg,
                  }}
                >
                  {row.t === "add" ? "+" : row.t === "del" ? "−" : " "}
                </span>
                <span style={{ color: fg }}>{row.text}</span>
              </div>
            )
          })}
        </div>
      ) : null}
    </div>
  )
}
