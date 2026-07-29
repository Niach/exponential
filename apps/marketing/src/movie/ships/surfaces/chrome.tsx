// surfaces/chrome.tsx — the desktop shell chrome, post-EXP-253/282 glass shell:
// TitleBar (34px macOS titlebar row hosting the center tab chips), ExpandedRail
// (the ONE labelled 164px rail — nav rows, boards inline, pinned user row),
// DockCollapsedStrip, CenterEmptyState.
// Pixel truth: the EXP-359 real-app reference screenshot + the desktop crates —
// crates/ui/src/surface.rs (tab_chip: h24, radius 10, FILL_ACTIVE when active),
// crates/ui/src/sidebar.rs (rail rows: FILL_ACTIVE pill, hover FILL_ROW, no
// marker bar; rail column FILL_SECTION wash), crates/ui/src/app_title_bar.rs
// (STROKE_ROW hairline under the tab row — EXP-288). All chrome strips are
// TRANSPARENT over the page gradient (EXP-269/277).
// Every component self-positions (position:absolute) at the contract's shell
// grid inside the 1568×980 window box — render them as direct children of
// WindowChassis. All frame values are COMPOSITION-GLOBAL.

import React from "react"
import { interpolate, interpolateColors, spring } from "remotion"
import { C, EASE, MONO_FONT, POP, R, UI_FONT, WIN } from "../theme"
import { IDENTITY } from "../fixtures"

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

const ChevronUpIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <Svg size={size} sw={2}>
    <path d="m18 15-6-6-6 6" />
  </Svg>
)

const PlusIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <Svg size={size} sw={2}>
    <path d="M5 12h14" />
    <path d="M12 5v14" />
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

const ZapIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size}>
    <path d="M4 14a1 1 0 0 1-.78-1.63l9.9-10.2a.5.5 0 0 1 .86.46l-1.92 6.02A1 1 0 0 0 13 10h7a1 1 0 0 1 .78 1.63l-9.9 10.2a.5.5 0 0 1-.86-.46l1.92-6.02A1 1 0 0 0 11 14z" />
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

const SquareTerminalIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <Svg size={size} sw={1.7}>
    <path d="m7 11 2-2-2-2" />
    <path d="M11 13h4" />
    <rect x="3" y="3" width="18" height="18" rx="2" />
  </Svg>
)

const XIcon: React.FC<{ size?: number }> = ({ size = 9 }) => (
  <Svg size={size} sw={2.2}>
    <path d="M18 6 6 18" />
    <path d="m6 6 12 12" />
  </Svg>
)

// Builtin status glyphs for tab chips (EXP-310: status icon + identifier ahead
// of the title). Pie-clock wedge for started statuses (icons.json progress-2-4).
export type TabStatus = "backlog" | "todo" | "in_progress" | "done"

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
    case "todo":
      return (
        <span style={{ color: C.statusTodo, display: "flex" }}>
          <Svg size={size} sw={2}>
            <circle cx="12" cy="12" r="10" />
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

// ── TitleBar (34px, transparent over the gradient, STROKE_ROW hairline) ───────
// Left: macOS traffic lights over the rail region. Right of the rail edge: the
// center tab strip — glass chips (surface.rs tab_chip: h24, radius 10, active =
// FILL_ACTIVE + white text, inactive transparent + muted). Optional presence
// facepile right-aligned (EXP-337 board-live clip).

export type ChromeTab = {
  id: string
  label: string
  mono?: boolean
  status?: TabStatus
  identifier?: string // mono shortcode ahead of the title (EXP-310)
}

// Deterministic tab-chip width so the assembler can aim the cursor.
export const chromeTabWidth = (t: ChromeTab): number => {
  let w = 8 + 8 // px padding
  if (t.status) w += 12 + 5
  if (t.identifier) w += Math.round(t.identifier.length * 6.7) + 5
  w += Math.round(t.label.length * (t.mono ? 7.3 : 6.3))
  w += 6 + 9 // gap + close glyph
  return Math.min(280, Math.max(72, w))
}

const TAB_STRIP_LEFT = WIN.rail + 10
const TAB_GAP = 4
const TAB_H = 24
const TAB_Y = (WIN.titleBar - TAB_H) / 2

// Window-local rect of a tab chip. Returns null when the id isn't present.
export const titleBarTabRect = (
  tabs: ChromeTab[],
  id: string
): { x: number; y: number; w: number; h: number } | null => {
  let x = TAB_STRIP_LEFT
  for (const t of tabs) {
    const w = chromeTabWidth(t)
    if (t.id === id) return { x, y: TAB_Y, w, h: TAB_H }
    x += w + TAB_GAP
  }
  return null
}

export type TitleBarPresence = {
  users: readonly { initials: string; color: string }[]
  at?: number // global frame the facepile pops in (staggered 3f/avatar); omit = always on
}

export type TitleBarProps = {
  frame: number
  tabs?: ChromeTab[]
  activeId?: string
  popAt?: Record<string, number> // tab id → global frame it POP-springs in (hidden before)
  presence?: TitleBarPresence
}

export const TitleBar: React.FC<TitleBarProps> = ({
  frame,
  tabs = [],
  activeId,
  popAt,
  presence,
}) => (
  <div
    style={{
      position: "absolute",
      top: 0,
      left: 0,
      right: 0,
      height: WIN.titleBar,
      boxSizing: "border-box",
      borderBottom: `1px solid ${C.strokeRow}`,
      display: "flex",
      alignItems: "center",
      fontFamily: UI_FONT,
      zIndex: 20,
    }}
  >
    {/* traffic lights */}
    {[
      { x: 20, c: "#ff5f57" },
      { x: 40, c: "#febc2e" },
      { x: 60, c: "#28c840" },
    ].map((l) => (
      <div
        key={l.c}
        style={{
          position: "absolute",
          left: l.x - 6,
          top: WIN.titleBar / 2 - 6,
          width: 12,
          height: 12,
          borderRadius: 999,
          backgroundColor: l.c,
        }}
      />
    ))}
    {/* tab chips */}
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
      {tabs.map((t) => {
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
              boxSizing: "border-box",
              display: "flex",
              alignItems: "center",
              gap: 5,
              padding: "0 8px",
              borderRadius: R.row,
              backgroundColor: isActive ? C.fillActive : "transparent",
              scale: String(scale),
              opacity,
            }}
          >
            {t.status ? <TabStatusGlyph status={t.status} size={12} /> : null}
            {t.identifier ? (
              <span
                style={{
                  fontFamily: MONO_FONT,
                  fontSize: 11,
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
                fontSize: 12,
                fontFamily: t.mono ? MONO_FONT : UI_FONT,
                fontWeight: isActive ? 500 : 400,
                color: isActive ? C.text : C.muted,
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
            >
              {t.label}
            </span>
            <span
              style={{
                color: isActive ? C.muted : C.dim,
                display: "flex",
                flex: "none",
              }}
            >
              <XIcon size={9} />
            </span>
          </div>
        )
      })}
    </div>
    {/* presence facepile — inline right of the tab chips so board-scoped
        camera crops keep it in shot (EXP-337 board-live) */}
    {presence ? (
      <div
        style={{
          position: "absolute",
          left:
            TAB_STRIP_LEFT +
            tabs.reduce((acc, t) => acc + chromeTabWidth(t) + TAB_GAP, 0) +
            14,
          top: 0,
          height: WIN.titleBar,
          display: "flex",
          alignItems: "center",
          gap: 7,
        }}
      >
        <div style={{ display: "flex" }}>
          {presence.users.map((u, i) => {
            const at =
              presence.at === undefined ? undefined : presence.at + i * 3
            if (at !== undefined && frame < at) return null
            const s =
              at === undefined
                ? 1
                : spring({
                    frame: frame - at,
                    fps: 30,
                    config: { damping: 12, stiffness: 200 },
                  })
            return (
              <span
                key={u.initials}
                style={{
                  width: 18,
                  height: 18,
                  marginLeft: i === 0 ? 0 : -5,
                  borderRadius: 999,
                  backgroundColor: `color-mix(in srgb, ${u.color} 26%, #18181b)`,
                  border: `1.5px solid #18181b`,
                  outline: `1px solid ${u.color}`,
                  color: u.color,
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                  fontSize: 8,
                  fontWeight: 600,
                  scale: String(0.6 + 0.4 * s),
                  zIndex: presence.users.length - i,
                }}
              >
                {u.initials}
              </span>
            )
          })}
        </div>
        <span
          style={{
            fontSize: 11.5,
            fontWeight: 500,
            color: C.muted,
            opacity:
              presence.at === undefined
                ? 1
                : interpolate(
                    frame,
                    [presence.at + 8, presence.at + 14],
                    [0, 1],
                    CLAMP
                  ),
          }}
        >
          {`${presence.users.length} online`}
        </span>
      </div>
    ) : null}
  </div>
)

// ── ExpandedRail (164px labelled rail — sidebar.rs RAIL_EXPANDED_W) ───────────
// Nav rows, a divider, the boards section (active board = FILL_ACTIVE pill),
// "New board", Files/Source Control, and the pinned bottom user row. The rail
// column carries the FILL_SECTION wash (sidebar.rs:1155); the glassier 0.72
// page alpha under it is painted by WindowChassis.

export type RailRowId =
  | "search"
  | "inbox"
  | "reviews"
  | "support"
  | "actions"
  | "board"
  | "new-board"
  | "files"
  | "source-control"
  | "user"

const ROW_H = 30
const ROW_X = 8
const ROW_W = WIN.rail - 16

// Window-local row top Ys. Nav rows from 42; divider; boards; divider;
// files/source-control; the user row pinned at the bottom.
const RAIL_ROW_Y: Record<RailRowId, number> = {
  search: 42,
  inbox: 74,
  reviews: 106,
  support: 138,
  actions: 170,
  board: 216,
  "new-board": 248,
  files: 294,
  "source-control": 326,
  user: WIN.h - 38,
}
const RAIL_DIVIDERS = [208, 286] // hairline Ys between the sections

// Cursor-targeting helper: window-local center of a rail row.
export const railRowCenter = (id: string): { x: number; y: number } => ({
  x: WIN.rail / 2,
  y: (RAIL_ROW_Y[id as RailRowId] ?? RAIL_ROW_Y.board) + ROW_H / 2,
})

const RAIL_ICON: Record<
  Exclude<RailRowId, "board" | "user">,
  React.FC<{ size?: number }>
> = {
  search: SearchIcon,
  inbox: InboxIcon,
  reviews: GitPullRequestIcon,
  support: LifeBuoyIcon,
  actions: ZapIcon,
  "new-board": PlusIcon,
  files: FolderIcon,
  "source-control": GitMergeIcon,
}

const RAIL_LABEL: Record<Exclude<RailRowId, "board" | "user">, string> = {
  search: "Search",
  inbox: "Inbox",
  reviews: "Reviews",
  support: "Support",
  actions: "Actions",
  "new-board": "New board",
  files: "Files",
  "source-control": "Source Control",
}

export type ExpandedRailProps = {
  frame: number
  active: string
  // Slides the FILL_ACTIVE pill + crossfades row tints from `from` to `active`,
  // starting at global frame `at` (10f, EASE). Resting state before `at` = `from`.
  activeTransition?: { from: string; at: number }
  dots?: string[] // rail row ids that carry a small dot at the row's right edge
  dotColor?: string
  boardName?: string
  boardGlyph?: "code" | "megaphone"
  userName?: string
  userInitial?: string
}

export const ExpandedRail: React.FC<ExpandedRailProps> = ({
  frame,
  active,
  activeTransition,
  dots = [],
  dotColor = C.synNumber,
  boardName = IDENTITY.project,
  boardGlyph = "code",
  userName = IDENTITY.user,
  userInitial = IDENTITY.initials,
}) => {
  const t = activeTransition
    ? interpolate(
        frame,
        [activeTransition.at, activeTransition.at + 10],
        [0, 1],
        { ...CLAMP, easing: EASE }
      )
    : 1
  const fromId = activeTransition?.from
  const toY = RAIL_ROW_Y[active as RailRowId] ?? RAIL_ROW_Y.board
  const fromY =
    fromId !== undefined ? (RAIL_ROW_Y[fromId as RailRowId] ?? toY) : toY
  const pillY = fromY + (toY - fromY) * t

  const fgOf = (id: RailRowId): string => {
    const activeFg = id === "board" ? C.text : C.text
    if (id === active && id === fromId) return activeFg
    if (id === active)
      return activeTransition
        ? interpolateColors(t, [0, 1], [C.muted, activeFg])
        : activeFg
    if (id === fromId) return interpolateColors(t, [0, 1], [activeFg, C.muted])
    return C.muted
  }

  const navRow = (id: Exclude<RailRowId, "board" | "user">) => {
    const Icon = RAIL_ICON[id]
    return (
      <div
        key={id}
        style={{
          position: "absolute",
          left: ROW_X,
          top: RAIL_ROW_Y[id],
          width: ROW_W,
          height: ROW_H,
          boxSizing: "border-box",
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "0 8px",
          borderRadius: R.row,
          color: id === "new-board" ? C.dim : fgOf(id),
        }}
      >
        <Icon size={14} />
        <span
          style={{
            flex: 1,
            fontSize: 12.5,
            fontWeight: 500,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {RAIL_LABEL[id]}
        </span>
        {dots.includes(id) ? (
          <span
            style={{
              width: 5,
              height: 5,
              flex: "none",
              borderRadius: 999,
              backgroundColor: dotColor,
            }}
          />
        ) : null}
      </div>
    )
  }

  return (
    <div
      style={{
        position: "absolute",
        top: WIN.titleBar,
        left: 0,
        bottom: 0,
        width: WIN.rail,
        boxSizing: "border-box",
        backgroundColor: C.fillSection,
        fontFamily: UI_FONT,
        zIndex: 10,
      }}
    >
      {/* the sliding FILL_ACTIVE pill (the fill IS the selection marker) */}
      <div
        style={{
          position: "absolute",
          left: ROW_X,
          top: pillY - WIN.titleBar,
          width: ROW_W,
          height: ROW_H,
          borderRadius: R.row,
          backgroundColor: C.fillActive,
        }}
      />
      {/* section hairlines */}
      {RAIL_DIVIDERS.map((y) => (
        <div
          key={y}
          style={{
            position: "absolute",
            left: 12,
            top: y - WIN.titleBar,
            width: WIN.rail - 24,
            height: 1,
            backgroundColor: C.strokeRow,
          }}
        />
      ))}
      {/* rows render in window-local coords shifted by the container top */}
      <div style={{ position: "absolute", inset: 0, translate: `0px ${-WIN.titleBar}px` }}>
        {(
          [
            "search",
            "inbox",
            "reviews",
            "support",
            "actions",
            "new-board",
            "files",
            "source-control",
          ] as const
        ).map(navRow)}
        {/* board row */}
        <div
          style={{
            position: "absolute",
            left: ROW_X,
            top: RAIL_ROW_Y.board,
            width: ROW_W,
            height: ROW_H,
            boxSizing: "border-box",
            display: "flex",
            alignItems: "center",
            gap: 8,
            padding: "0 8px",
            borderRadius: R.row,
            color: fgOf("board"),
          }}
        >
          <span style={{ color: C.indigoSoft, display: "flex" }}>
            {boardGlyph === "code" ? (
              <CodeIcon size={14} />
            ) : (
              <MegaphoneIcon size={14} />
            )}
          </span>
          <span
            style={{
              flex: 1,
              fontSize: 12.5,
              fontWeight: 500,
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
            }}
          >
            {boardName}
          </span>
          {dots.includes("board") ? (
            <span
              style={{
                width: 5,
                height: 5,
                flex: "none",
                borderRadius: 999,
                backgroundColor: dotColor,
              }}
            />
          ) : null}
        </div>
        {/* pinned bottom: user + settings gear */}
        <div
          style={{
            position: "absolute",
            left: ROW_X,
            top: RAIL_ROW_Y.user,
            width: ROW_W,
            height: ROW_H,
            boxSizing: "border-box",
            display: "flex",
            alignItems: "center",
            gap: 8,
            padding: "0 8px",
            borderRadius: R.row,
            color: C.muted,
          }}
        >
          <span
            style={{
              width: 18,
              height: 18,
              flex: "none",
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              borderRadius: 999,
              backgroundColor: "rgba(192,38,211,0.28)",
              border: "1px solid rgba(217,70,239,0.55)",
              color: "#e879f9",
              fontSize: 9,
              fontWeight: 600,
            }}
          >
            {userInitial}
          </span>
          <span
            style={{
              flex: 1,
              fontSize: 12.5,
              fontWeight: 500,
              color: C.text,
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
            }}
          >
            {userName}
          </span>
          <SettingsIcon size={14} />
        </div>
      </div>
    </div>
  )
}

// ── DockCollapsedStrip (29px bottom strip: ▤ Terminal (1) … ⌃) ───────────────
export type DockCollapsedStripProps = {
  frame: number
  count?: number
}

export const DockCollapsedStrip: React.FC<DockCollapsedStripProps> = ({
  count = 1,
}) => (
  <div
    style={{
      position: "absolute",
      left: WIN.rail,
      right: 0,
      bottom: 0,
      height: WIN.dockStrip,
      boxSizing: "border-box",
      borderTop: `1px solid ${C.strokeRow}`,
      display: "flex",
      alignItems: "center",
      padding: "0 10px 0 12px",
      gap: 8,
      fontFamily: UI_FONT,
      zIndex: 10,
    }}
  >
    <span style={{ color: C.muted, display: "flex" }}>
      <SquareTerminalIcon size={13} />
    </span>
    <span
      style={{ fontSize: 12, fontWeight: 500, color: C.muted }}
    >{`Terminal (${count})`}</span>
    <div style={{ flex: 1 }} />
    <span style={{ color: C.muted, display: "flex" }}>
      <ChevronUpIcon size={13} />
    </span>
  </div>
)

// ── CenterEmptyState ("Nothing open") ────────────────────────────────────────
export type CenterEmptyStateProps = {
  frame: number
  bottom?: number // window-local inset from the window's bottom edge (default: collapsed dock strip)
  // WINDOW-LOCAL point to center the icon+text block on. A zoomed-in camera
  // crops the pane, so pane-centering can land the block half off-frame
  // (EXP-217) — callers pass the visible region's center instead.
  contentCenter?: { x: number; y: number }
}

export const CenterEmptyState: React.FC<CenterEmptyStateProps> = ({
  bottom = WIN.dockStrip,
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
        Pick an issue from the sidebar — it opens as a tab here.
      </div>
    </>
  )
  const paneLeft = WIN.rail + WIN.sidebar
  const paneTop = WIN.titleBar
  return (
    <div
      style={{
        position: "absolute",
        left: paneLeft,
        right: 0,
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
  dockStripChevron: { x: WIN.w - 17, y: WIN.h - WIN.dockStrip / 2 },
  dockStripLabel: { x: WIN.rail + 60, y: WIN.h - WIN.dockStrip / 2 },
} as const
