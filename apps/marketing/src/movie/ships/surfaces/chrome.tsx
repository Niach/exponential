// surfaces/chrome.tsx — the desktop shell chrome, post-EXP-870/877 glass
// shell: TitleBar (the 44px work-tabs band hosting the tab chips — the live
// runs lead in their agent group), CompactRail (the left column's fixed
// header over the rail FOLDED to its 48px icon column), ListNav (the list an
// open detail was picked from, beside it), CutoutPanel (EXP-723's rounded
// working surface the main view renders INTO) and CenterEmptyState.
// Pixel truth: shots/{board,issue-detail,steering}/desktop.webp + the desktop
// crates — crates/ui/src/shell.rs (COMPACT_RAIL_WIDTH 48, LEFT_COLUMN_WIDTH
// 272, PANEL_MARGIN_TOP 0), crates/ui/src/sidebar.rs (rail_compact_button:
// 32px squares, FILL_ACTIVE when active; ListPanel's nav rows),
// crates/ui/src/app_title_bar.rs (WORK_TABS_BAND_H 44) and screens.rs (the
// agent-grouped live tabs). All chrome strips are TRANSPARENT over the page
// gradient (EXP-269/277).
// Every component self-positions (position:absolute) at the contract's shell
// grid inside the 1568×980 window box — render them as direct children of
// WindowChassis. All frame values are COMPOSITION-GLOBAL.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, MONO_FONT, POP, R, UI_FONT, WIN } from "../theme"
import { IDENTITY } from "../fixtures"
import { ClaudeMark } from "../../closedloop/surfaces/agentmarks"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

// ── Tiny inline icons (lucide-style, stroke 1.6–2, currentColor) ──────────────
const Svg: React.FC<{
  size: number
  sw?: number
  children: React.ReactNode
}> = ({ size, sw = 1.8, children }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={sw}
    strokeLinecap="round"
    strokeLinejoin="round"
    style={{ display: "block", flexShrink: 0 }}
  >
    {children}
  </svg>
)

const CodeIcon: React.FC<{ size?: number }> = ({ size = 15 }) => (
  <Svg size={size} sw={2}>
    <path d="m16 18 6-6-6-6" />
    <path d="m8 6-6 6 6 6" />
  </Svg>
)

const MegaphoneIcon: React.FC<{ size?: number }> = ({ size = 15 }) => (
  <Svg size={size} sw={2}>
    <path d="m3 11 18-5v12L3 14v-3z" />
    <path d="M11.6 16.8a3 3 0 1 1-5.8-1.6" />
  </Svg>
)

const SearchIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size}>
    <circle cx="11" cy="11" r="8" />
    <path d="m21 21-4.3-4.3" />
  </Svg>
)

const InboxIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size}>
    <polyline points="22 12 16 12 14 15 10 15 8 12 2 12" />
    <path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />
  </Svg>
)

const GitPullRequestIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size}>
    <circle cx="18" cy="18" r="3" />
    <circle cx="6" cy="6" r="3" />
    <path d="M13 6h3a2 2 0 0 1 2 2v7" />
    <path d="M6 9v12" />
  </Svg>
)

const LifeBuoyIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size}>
    <circle cx="12" cy="12" r="10" />
    <path d="m4.93 4.93 4.24 4.24" />
    <path d="m14.83 9.17 4.24-4.24" />
    <path d="m14.83 14.83 4.24 4.24" />
    <path d="m9.17 14.83-4.24 4.24" />
    <circle cx="12" cy="12" r="4" />
  </Svg>
)

const BotIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size} sw={1.7}>
    <path d="M12 8V4H8" />
    <rect x="4" y="8" width="16" height="12" rx="2" />
    <path d="M2 14h2" />
    <path d="M20 14h2" />
    <path d="M15 13v2" />
    <path d="M9 13v2" />
  </Svg>
)

// nav-devices = lucide `monitor`, nav-automations = lucide `zap`
// (packages/icons/icons.json) — the two rail entries EXP-686 split out of the
// old "Agents" row.
const MonitorIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size} sw={1.7}>
    <rect x="2" y="3" width="20" height="14" rx="2" />
    <path d="M8 21h8" />
    <path d="M12 17v4" />
  </Svg>
)

// action-chat = message-circle, nav-terminal = square-terminal (packages/icons/icons.json).
const MessageCircleIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size} sw={1.7}>
    <path d="M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.412-.961a2 2 0 0 1 1.099.092 10 10 0 1 0-4.776-4.756" />
  </Svg>
)

const SquareTerminalIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size} sw={1.7}>
    <path d="m7 11 2-2-2-2" />
    <path d="M11 13h4" />
    <rect x="3" y="3" width="18" height="18" rx="2" />
  </Svg>
)

const ZapIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size} sw={1.7}>
    <path d="M4 14a1 1 0 0 1-.78-1.63l9.9-10.2a.5.5 0 0 1 .86.46l-1.92 6.02A1 1 0 0 0 13 10h7a1 1 0 0 1 .78 1.63l-9.9 10.2a.5.5 0 0 1-.86-.46l1.92-6.02A1 1 0 0 0 11 14z" />
  </Svg>
)

const SquareKanbanIcon: React.FC<{ size?: number }> = ({ size = 15 }) => (
  <Svg size={size} sw={2}>
    <rect x="3" y="3" width="18" height="18" rx="2" />
    <path d="M8 7v7" />
    <path d="M12 7v4" />
    <path d="M16 7v9" />
  </Svg>
)

// nav-team-switcher = chevrons-up-down, nav-create-issue = square-pen
// (icons.json) — the EXP-723 rail header.
const ChevronsUpDownIcon: React.FC<{ size?: number }> = ({ size = 11 }) => (
  <Svg size={size} sw={2}>
    <path d="m7 15 5 5 5-5" />
    <path d="m7 9 5-5 5 5" />
  </Svg>
)
const SquarePenIcon: React.FC<{ size?: number }> = ({ size = 15 }) => (
  <Svg size={size} sw={2}>
    <path d="M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
    <path d="M18.375 2.625a1 1 0 0 1 3 3l-9.013 9.014a2 2 0 0 1-.853.505l-2.873.84a.5.5 0 0 1-.62-.62l.84-2.873a2 2 0 0 1 .506-.852z" />
  </Svg>
)

const CircleXIcon: React.FC<{ size?: number }> = ({ size = 12 }) => (
  <Svg size={size} sw={2}>
    <circle cx="12" cy="12" r="10" />
    <path d="m15 9-6 6" />
    <path d="m9 9 6 6" />
  </Svg>
)

const FolderIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size}>
    <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
  </Svg>
)

const GitMergeIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size}>
    <circle cx="18" cy="18" r="3" />
    <circle cx="6" cy="6" r="3" />
    <path d="M6 21V9a9 9 0 0 0 9 9" />
  </Svg>
)

const SettingsIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size} sw={1.6}>
    <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
    <circle cx="12" cy="12" r="3" />
  </Svg>
)

const ChevronLeftIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <Svg size={size} sw={2}>
    <path d="m15 18-6-6 6-6" />
  </Svg>
)

// Builtin status glyphs for tab chips (EXP-310: status icon + identifier ahead
// of the title). Pie-clock wedge for started statuses (icons.json progress-2-4).
export type TabStatus = "backlog" | "in_progress" | "done"

const TabStatusGlyph: React.FC<{ status: TabStatus; size?: number }> = ({
  status,
  size = 12,
}) => {
  switch (status) {
    case "backlog":
      return (
        <span style={{ color: C.statusBacklog, display: "flex" }}>
          <Svg size={size} sw={2}>
            <circle cx="12" cy="12" r="10" strokeDasharray="3.6 3.4" />
          </Svg>
        </span>
      )
    case "in_progress":
      return (
        <span style={{ color: C.statusInProgress, display: "flex" }}>
          <svg
            width={size}
            height={size}
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
            style={{ display: "block", flexShrink: 0 }}
          >
            <circle cx="12" cy="12" r="10" />
            <path d="M12 12 L12 6 A6 6 0 0 1 12 18 Z" fill="currentColor" stroke="none" />
          </svg>
        </span>
      )
    case "done":
      return (
        <span style={{ color: C.statusDone, display: "flex" }}>
          <Svg size={size} sw={2}>
            <circle cx="12" cy="12" r="10" />
            <path d="m9 12 2 2 4-4" />
          </Svg>
        </span>
      )
  }
}

// ── TitleBar (the 44px work-tabs band — bare ground, EXP-723/877) ────────────
// Left: macOS traffic lights over the left column's 34px strip. Over the
// panel: the tab strip — glass chips (32px, radius 10, active = FILL_ACTIVE +
// white text, inactive transparent + muted; app_title_bar.rs
// WORK_TABS_BAND_H). EXP-870/877: an issue and its run are ONE tab, and every
// live run of mine leads the strip in an AGENT GROUP — the agent's brand mark
// (Claude's in its orange), the run chips wearing their steady liveness dot
// (never a spinner, screens.rs ChipLead::Dot), and the group's collapse
// chevron. Ordinary tabs follow with their status glyph. The band carries no
// fill and no hairline — that is what makes the panel under it read as a
// cutout.

export type ChromeTab = {
  id: string
  label: string
  mono?: boolean
  status?: TabStatus
  identifier?: string // mono shortcode ahead of the title (EXP-310)
  /** EXP-870: a live run of mine — the chip joins the agent group and its
   * lead is the liveness dot in this tone instead of the status glyph. */
  liveDot?: string
}

// Deterministic tab-chip width so the assembler can aim the cursor.
export const chromeTabWidth = (t: ChromeTab): number => {
  let w = 10 + 10 // px padding
  if (t.liveDot) w += 7 + 7
  else if (t.status) w += 13 + 7
  if (t.identifier) w += Math.round(t.identifier.length * 7.2) + 7
  w += Math.round(t.label.length * (t.mono ? 7.9 : 7.1))
  return Math.min(300, Math.max(80, w))
}

const TAB_H = 32
const TAB_GAP = 4
const TAB_Y = (WIN.titleBar - TAB_H) / 2
const TAB_STRIP_LEFT = WIN.panel.x + 8
const GROUP_MARK = 15
const GROUP_GAP = 8
const COLLAPSE_W = 20

const orderTabs = (tabs: ChromeTab[]) => [
  ...tabs.filter((t) => t.liveDot),
  ...tabs.filter((t) => !t.liveDot),
]

// Window-local rect of a tab chip. Returns null when the id isn't present.
export const titleBarTabRect = (
  tabs: ChromeTab[],
  id: string
): { x: number; y: number; w: number; h: number } | null => {
  const ordered = orderTabs(tabs)
  const live = ordered.filter((t) => t.liveDot)
  let x = TAB_STRIP_LEFT + (live.length > 0 ? GROUP_MARK + GROUP_GAP : 0)
  for (const [i, t] of ordered.entries()) {
    if (i === live.length && live.length > 0) x += COLLAPSE_W + GROUP_GAP * 1.5
    const w = chromeTabWidth(t)
    if (t.id === id) return { x, y: TAB_Y, w, h: TAB_H }
    x += w + TAB_GAP
  }
  return null
}

export type TitleBarProps = {
  frame: number
  tabs?: ChromeTab[]
  activeId?: string
  popAt?: Record<string, number> // tab id → global frame it POP-springs in (hidden before)
}

export const TitleBar: React.FC<TitleBarProps> = ({
  frame,
  tabs = [],
  activeId,
  popAt,
}) => {
  const ordered = orderTabs(tabs)
  const live = ordered.filter((t) => t.liveDot)
  const plain = ordered.filter((t) => !t.liveDot)

  const chip = (t: ChromeTab) => {
    const at = popAt?.[t.id]
    if (at !== undefined && frame < at) return null
    let scale = 1
    let opacity = 1
    if (at !== undefined) {
      const s = spring({ frame: frame - at, fps: 30, config: POP })
      scale = 0.75 + 0.25 * s
      opacity = interpolate(frame, [at, at + 3], [0, 1], CLAMP)
    }
    const isActive = t.id === activeId
    return (
      <div
        key={t.id}
        style={{
          width: chromeTabWidth(t),
          height: TAB_H,
          flex: "none",
          boxSizing: "border-box",
          display: "flex",
          alignItems: "center",
          gap: 7,
          padding: "0 10px",
          borderRadius: R.row,
          backgroundColor: isActive ? C.fillActive : "transparent",
          scale: String(scale),
          opacity,
        }}
      >
        {t.liveDot ? (
          <span
            style={{
              width: 7,
              height: 7,
              flex: "none",
              borderRadius: 999,
              backgroundColor: t.liveDot,
            }}
          />
        ) : t.status ? (
          <TabStatusGlyph status={t.status} size={13} />
        ) : null}
        {t.identifier ? (
          <span
            style={{
              fontFamily: MONO_FONT,
              fontSize: 12,
              color: C.muted,
              whiteSpace: "nowrap",
            }}
          >
            {t.identifier}
          </span>
        ) : null}
        <span
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 14,
            fontFamily: t.mono ? MONO_FONT : UI_FONT,
            fontWeight: 400,
            color: isActive ? C.text : C.muted,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {t.label}
        </span>
      </div>
    )
  }

  return (
    <div
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        right: 0,
        height: WIN.titleBar,
        boxSizing: "border-box",
        display: "flex",
        alignItems: "center",
        fontFamily: UI_FONT,
        zIndex: 20,
      }}
    >
      {/* traffic lights, over the left column's 34px strip */}
      {[
        { x: 16, c: "#ff5f57" },
        { x: 39, c: "#febc2e" },
        { x: 62, c: "#28c840" },
      ].map((l) => (
        <div
          key={l.c}
          style={{
            position: "absolute",
            left: l.x - 6,
            top: WIN.leftStrip / 2 - 6,
            width: 12,
            height: 12,
            borderRadius: 999,
            backgroundColor: l.c,
          }}
        />
      ))}
      {/* the tab strip: the agent group (mark · live chips · collapse), then
          the ordinary chips */}
      <div
        style={{
          position: "absolute",
          left: TAB_STRIP_LEFT,
          top: TAB_Y,
          height: TAB_H,
          display: "flex",
          gap: TAB_GAP,
          alignItems: "center",
        }}
      >
        {live.length > 0 ? (
          <>
            <span style={{ display: "flex", marginRight: GROUP_GAP - TAB_GAP }}>
              <ClaudeMark size={GROUP_MARK} />
            </span>
            {live.map(chip)}
            <span
              style={{
                width: COLLAPSE_W,
                display: "flex",
                justifyContent: "center",
                color: C.muted,
                marginRight: GROUP_GAP * 1.5 - TAB_GAP,
              }}
            >
              <ChevronLeftIcon size={13} />
            </span>
          </>
        ) : null}
        {plain.map(chip)}
      </div>
    </div>
  )
}

// ── CutoutPanel (EXP-723, shell.rs) ──────────────────────────────────────────
// The working surface: a rounded card flush under the band (EXP-877) and
// 10px in from the other sides, radius::LG, the card hairline, the
// translucent FILL_PANEL wash, overflow hidden so its corners clip whatever
// it holds. The main view keeps its window-local coordinates and renders
// INTO it — the inner box is the whole window, offset back, so the panel
// simply clips.
export const CutoutPanel: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => (
  <div
    style={{
      position: "absolute",
      left: WIN.panel.x,
      top: WIN.panel.y,
      width: WIN.panel.w,
      height: WIN.panel.h,
      boxSizing: "border-box",
      borderRadius: WIN.panel.radius,
      border: `1px solid ${C.strokeCard}`,
      backgroundColor: C.fillPanel,
      overflow: "hidden",
      zIndex: 5,
    }}
  >
    <div
      style={{
        position: "absolute",
        left: -(WIN.panel.x + 1),
        top: -(WIN.panel.y + 1),
        width: WIN.w,
        height: WIN.h,
      }}
    >
      {children}
    </div>
  </div>
)

// ── CompactRail (EXP-870: the rail folded beside a list — sidebar.rs) ───────
// While a detail sits beside the list it was picked from, the left column is
// the FIXED header (team switcher · Search · New issue, full column width)
// over two slots: the rail folded to its 48px ICON column and the 272px
// ListNav. The icon column keeps every destination in the expanded rail's
// order — Inbox / Support / Devices / Actions / Automations / Reviews / Agent
// (its badge counts my live runs) · the boards · Files / Source Control —
// as 32px squares (FILL_ACTIVE when active, badges pinned top-right), minus
// everything that needs a label (section labels, What's new, Getting
// started); the footer stacks the new-terminal button, the gear and the
// avatar. The Sessions section is gone (EXP-870): live runs are tabs.

export type RailRowId =
  | "inbox"
  | "support"
  | "devices"
  | "actions"
  | "automations"
  | "reviews"
  | "agent"
  | "board"
  | "board1"
  | "board2"
  | "files"
  | "source-control"

const ICON_SQ = 32
const ICON_X = (WIN.rail - ICON_SQ) / 2 // 8
const ICON_GAP = 3.5 // gap_1
const PITCH = ICON_SQ + ICON_GAP
const HEADER_Y = WIN.leftStrip
const HEADER_H = WIN.leftHeader
const COLUMN_TOP = HEADER_Y + HEADER_H + 8 // under the header's hairline
const DIVIDER_BLOCK = 1 + 2 * ICON_GAP + ICON_GAP // my_1 hairline + gap

const railIconY: Record<RailRowId, number> = (() => {
  const nav: RailRowId[] = [
    "inbox",
    "support",
    "devices",
    "actions",
    "automations",
    "reviews",
    "agent",
  ]
  const out = {} as Record<RailRowId, number>
  let y = COLUMN_TOP
  for (const id of nav) {
    out[id] = y
    y += PITCH
  }
  y += DIVIDER_BLOCK
  for (const id of ["board", "board1", "board2"] as const) {
    out[id] = y
    y += PITCH
  }
  y += DIVIDER_BLOCK
  for (const id of ["files", "source-control"] as const) {
    out[id] = y
    y += PITCH
  }
  return out
})()
const RAIL_DIVIDERS = [
  railIconY.board - ICON_GAP - DIVIDER_BLOCK / 2,
  railIconY.files - ICON_GAP - DIVIDER_BLOCK / 2,
]

// Cursor-targeting helper: window-local center of a rail icon.
export const railRowCenter = (id: string): { x: number; y: number } => ({
  x: WIN.rail / 2,
  y: (railIconY[id as RailRowId] ?? railIconY.board) + ICON_SQ / 2,
})

type NavRowId = Exclude<RailRowId, "board" | "board1" | "board2">

const RAIL_ICON: Record<NavRowId, React.FC<{ size?: number }>> = {
  inbox: InboxIcon,
  support: LifeBuoyIcon,
  devices: MonitorIcon,
  actions: BotIcon,
  automations: ZapIcon,
  reviews: GitPullRequestIcon,
  agent: MessageCircleIcon,
  files: FolderIcon,
  "source-control": GitMergeIcon,
}

// Board glyphs are the pickable icons.json names the boards actually carry;
// each keeps its own accent (the rail is the only colored thing in the shell).
export type BoardGlyph = "code" | "kanban" | "megaphone"
const BOARD_GLYPH: Record<BoardGlyph, React.FC<{ size?: number }>> = {
  code: CodeIcon,
  kanban: SquareKanbanIcon,
  megaphone: MegaphoneIcon,
}

export type RailBoard = { name: string; glyph: BoardGlyph; color: string }

// The two companion boards every rail shows beside the film's own board — the
// product's Boards group is never a single row.
const COMPANION_BOARDS: RailBoard[] = [
  { name: "Launch Marketing", glyph: "kanban", color: "#f59e0b" },
  { name: "Product Feedback", glyph: "megaphone", color: "#22c55e" },
]

// Text sizes at the 14px rem, scaled to the 1568-wide window like the rest of
// the ship (text_sm 12.25 → 13.5, text_xs 10.5 → 11.5).
const TEXT_SM = 13.5

export type CompactRailProps = {
  frame: number
  active: string
  dots?: string[] // rail icon ids that carry a small badge dot
  dotColor?: string
  /** EXP-870: the Agent entry's live-run count (hidden at 0). */
  agentCount?: number
  teamName?: string
  boardName?: string
  boardGlyph?: BoardGlyph
  boards?: RailBoard[] // full override of the Boards group
  userName?: string
  userInitial?: string
}

export const CompactRail: React.FC<CompactRailProps> = ({
  active,
  dots = [],
  dotColor = C.green,
  agentCount = 0,
  teamName = IDENTITY.team,
  boardName = IDENTITY.project,
  boardGlyph = "code",
  boards,
  userInitial = IDENTITY.initials,
}) => {
  const boardRows: RailBoard[] = boards ?? [
    { name: boardName, glyph: boardGlyph, color: "#818cf8" },
    ...COMPANION_BOARDS,
  ]

  const badge = (id: RailRowId): React.ReactNode => {
    if (id === "agent" && agentCount > 0) {
      return (
        <span
          style={{
            position: "absolute",
            top: 1,
            right: 0,
            height: 14,
            minWidth: 14,
            boxSizing: "border-box",
            padding: "0 3px",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            borderRadius: 999,
            backgroundColor: C.green,
            color: C.canvas,
            fontSize: 10,
            fontWeight: 600,
          }}
        >
          {agentCount}
        </span>
      )
    }
    if (id === "source-control") {
      return (
        <span
          style={{
            position: "absolute",
            top: 2,
            right: 2,
            color: C.destructive,
            display: "flex",
          }}
        >
          <CircleXIcon size={10} />
        </span>
      )
    }
    if (!dots.includes(id)) return null
    return (
      <span
        style={{
          position: "absolute",
          top: 5,
          right: 5,
          width: 6,
          height: 6,
          borderRadius: 999,
          backgroundColor: dotColor,
        }}
      />
    )
  }

  const square = (id: RailRowId, glyph: React.ReactNode) => (
    <div
      key={id}
      style={{
        position: "absolute",
        left: ICON_X,
        top: railIconY[id],
        width: ICON_SQ,
        height: ICON_SQ,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        borderRadius: R.row,
        backgroundColor: id === active ? C.fillActive : "transparent",
        color: C.text,
      }}
    >
      {glyph}
      {badge(id)}
    </div>
  )

  const iconBtn = (
    child: React.ReactNode,
    style: React.CSSProperties = {}
  ): React.ReactElement => (
    <span
      style={{
        width: ICON_SQ,
        height: ICON_SQ,
        flex: "none",
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        borderRadius: 999,
        color: C.text,
        ...style,
      }}
    >
      {child}
    </span>
  )

  return (
    <div
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        bottom: 0,
        width: WIN.leftColumn,
        // EXP-767: the column paints NOTHING of its own — it sits on the one
        // page gradient the shell root paints, and the cutout panel is the
        // window's only brightness step.
        fontFamily: UI_FONT,
        zIndex: 10,
      }}
    >
      {/* render_left_column_header (EXP-723/870): FIXED over both slots —
          team switcher · Search · New issue */}
      <div
        style={{
          position: "absolute",
          left: 8,
          right: 8,
          top: HEADER_Y,
          height: HEADER_H,
          display: "flex",
          alignItems: "center",
          gap: ICON_GAP,
        }}
      >
        <div
          style={{
            flex: 1,
            minWidth: 0,
            height: HEADER_H,
            display: "flex",
            alignItems: "center",
            gap: 7,
            padding: "0 5.25px",
            color: C.text,
          }}
        >
          <span
            style={{
              width: 28,
              height: 28,
              flex: "none",
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              borderRadius: 7,
              backgroundColor: "#e5e5e5",
              color: "#171717",
              fontSize: 15,
              fontWeight: 600,
            }}
          >
            {teamName.charAt(0)}
          </span>
          <span
            style={{
              flex: 1,
              minWidth: 0,
              fontSize: TEXT_SM,
              fontWeight: 600,
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
            }}
          >
            {teamName}
          </span>
          <span style={{ color: C.muted, display: "flex", flex: "none" }}>
            <ChevronsUpDownIcon size={11} />
          </span>
        </div>
        {iconBtn(<SearchIcon size={15} />)}
        {iconBtn(<SquarePenIcon size={15} />, {
          backgroundColor: "#e5e5e5",
          color: "#171717",
        })}
      </div>
      <div
        style={{
          position: "absolute",
          left: 8,
          right: 8,
          top: HEADER_Y + HEADER_H + 3.5,
          height: 1,
          backgroundColor: C.strokeRow,
        }}
      />

      {/* the icon column */}
      {(
        [
          "inbox",
          "support",
          "devices",
          "actions",
          "automations",
          "reviews",
          "agent",
          "files",
          "source-control",
        ] as const
      ).map((id) => square(id, React.createElement(RAIL_ICON[id], { size: 16 })))}
      {boardRows.slice(0, 3).map((b, i) => {
        const id = (i === 0 ? "board" : `board${i}`) as RailRowId
        return square(
          id,
          <span style={{ color: b.color, display: "flex" }}>
            {React.createElement(BOARD_GLYPH[b.glyph], { size: 16 })}
          </span>
        )
      })}
      {RAIL_DIVIDERS.map((y) => (
        <div
          key={y}
          style={{
            position: "absolute",
            left: ICON_X,
            top: y,
            width: ICON_SQ,
            height: 1,
            backgroundColor: C.strokeCard,
          }}
        />
      ))}
      {/* the footer, stacked: new terminal · settings · the avatar */}
      <div
        style={{
          position: "absolute",
          left: 0,
          width: WIN.rail,
          bottom: 8,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: ICON_GAP,
          color: C.muted,
        }}
      >
        {iconBtn(<SquareTerminalIcon size={15} />, { color: C.muted })}
        {iconBtn(<SettingsIcon size={15} />, { color: C.muted })}
        {iconBtn(
          <span
            style={{
              width: 24,
              height: 24,
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              borderRadius: 999,
              backgroundColor: "rgba(59,130,246,0.28)",
              color: "#93c5fd",
              fontSize: 10,
              fontWeight: 600,
            }}
          >
            {userInitial}
          </span>
        )}
      </div>
    </div>
  )
}

// ── ListNav (EXP-851/863/870 — sidebar.rs ListPanel in ListMode::Nav) ────────
// The list an open detail was picked from, beside the folded rail: a 40px
// BACK row (the bare back chevron + the list's name, semibold) over its rule,
// then the SIMPLIFIED list the caller passes as children (BoardTool at its
// `nav` density). The slot's left hairline (border_l STROKE_ROW) only exists
// while a panel is up.
export const LIST_NAV_BODY_TOP = HEADER_Y + HEADER_H + 3.5 + 1 + 3.5 + 40 + 8

export const ListNav: React.FC<{
  title: string
  children: React.ReactNode
}> = ({ title, children }) => (
  <div
    style={{
      position: "absolute",
      left: WIN.rail,
      top: HEADER_Y + HEADER_H + 8,
      width: WIN.listNav,
      bottom: 0,
      borderLeft: `1px solid ${C.strokeRow}`,
      fontFamily: UI_FONT,
      zIndex: 10,
    }}
  >
    <div
      style={{
        position: "absolute",
        left: 8,
        right: 8,
        top: 0,
        height: 40,
        boxSizing: "border-box",
        display: "flex",
        alignItems: "center",
        gap: 7,
        padding: "0 7px",
        color: C.text,
      }}
    >
      <span style={{ display: "flex", color: C.text }}>
        <ChevronLeftIcon size={16} />
      </span>
      <span style={{ fontSize: TEXT_SM, fontWeight: 600 }}>{title}</span>
    </div>
    <div
      style={{
        position: "absolute",
        left: 8,
        right: 8,
        top: 43.5,
        height: 1,
        backgroundColor: C.strokeRow,
      }}
    />
    <div
      style={{
        position: "absolute",
        left: 0,
        right: 0,
        top: LIST_NAV_BODY_TOP - (HEADER_Y + HEADER_H + 8),
        bottom: 0,
      }}
    >
      {children}
    </div>
  </div>
)

// ── CenterEmptyState ("Nothing open") ────────────────────────────────────────
export type CenterEmptyStateProps = {
  frame: number
  bottom?: number // window-local inset from the window's bottom edge (default: the panel margin)
  // WINDOW-LOCAL point to center the icon+text block on. A zoomed-in camera
  // crops the pane, so pane-centering can land the block half off-frame
  // (EXP-217) — callers pass the visible region's center instead.
  contentCenter?: { x: number; y: number }
}

export const CenterEmptyState: React.FC<CenterEmptyStateProps> = ({
  bottom = WIN.h - WIN.panel.bottom,
  contentCenter,
}) => {
  const content = (
    <>
      <span style={{ color: C.dim, display: "flex" }}>
        <InboxIcon size={24} />
      </span>
      <div
        style={{ marginTop: 10, fontSize: 13, fontWeight: 500, color: C.text }}
      >
        Nothing open
      </div>
      <div style={{ marginTop: 4, fontSize: 12, color: C.muted }}>
        Pick an issue from the sidebar. It opens as a tab here.
      </div>
    </>
  )
  const paneLeft = WIN.panel.x
  const paneTop = WIN.panel.y
  return (
    <div
      style={{
        position: "absolute",
        left: paneLeft,
        right: WIN.w - WIN.panel.right,
        top: paneTop,
        bottom,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: contentCenter ? "flex-start" : "center",
        fontFamily: UI_FONT,
      }}
    >
      {contentCenter ? (
        <div
          style={{
            position: "absolute",
            left: contentCenter.x - paneLeft,
            top: contentCenter.y - paneTop,
            translate: "-50% -50%",
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
          }}
        >
          {content}
        </div>
      ) : (
        content
      )}
    </div>
  )
}

// ── Cursor anchors (window-local; text is auto-laid-out, treat as ±4px) ──────
export const CHROME_ANCHORS = {
  trafficLights: { x: 40, y: 17 },
  tabStripStart: { x: TAB_STRIP_LEFT + 40, y: WIN.titleBar / 2 },
} as const
