// surfaces/chrome.tsx — the desktop shell chrome: TopBar, IconRail, TabsBar,
// DockCollapsedStrip, CenterEmptyState.
// Pixel truth: ref/desktop-hero-board-issue.png + ref/desktop-claude-session-dock.png.
// Measured off the refs: ALL chrome strips (top bar / rail / tab strip / collapsed
// dock strip) sit on #171717 (C.panel) — the app's title_bar/tab_bar token — while
// the sidebar + center content sit on #0a0a0a. Active center tab bg = #262626
// (C.accentBg); rail active = icon tinted C.indigoSoft + subtle bg + 2px rounded
// indigo bar hugging the rail's left edge; exactly ONE rail divider (after search).
// Every component self-positions (position:absolute) at the contract's shell grid
// inside the 1568×980 window box — render them as direct children of WindowChassis.
// All frame values are COMPOSITION-GLOBAL.

import React from "react"
import { interpolate, interpolateColors, spring } from "remotion"
import { C, EASE, MONO_FONT, POP, UI_FONT, WIN } from "../theme"
import { IDENTITY } from "../fixtures"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

// ── Tiny inline icons (lucide-style, stroke 1.6–2, currentColor) ──────────────
const Svg: React.FC<{ size: number; sw?: number; children: React.ReactNode }> = ({ size, sw = 1.8, children }) => (
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

const GlobeIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <Svg size={size} sw={1.7}>
    <circle cx="12" cy="12" r="10" />
    <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
    <path d="M2 12h20" />
  </Svg>
)

const ChevronsUpDownIcon: React.FC<{ size?: number }> = ({ size = 12 }) => (
  <Svg size={size} sw={2}>
    <path d="m7 15 5 5 5-5" />
    <path d="m7 9 5-5 5 5" />
  </Svg>
)

const ChevronDownIcon: React.FC<{ size?: number }> = ({ size = 11 }) => (
  <Svg size={size} sw={2}>
    <path d="m6 9 6 6 6-6" />
  </Svg>
)

const ChevronUpIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <Svg size={size} sw={2}>
    <path d="m18 15-6-6-6 6" />
  </Svg>
)

const PlayIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <Svg size={size} sw={2}>
    <path d="M6 4.5 19.5 12 6 19.5Z" />
  </Svg>
)

const CheckIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <Svg size={size} sw={2}>
    <path d="M20 6 9 17l-5-5" />
  </Svg>
)

const ArrowUpIcon: React.FC<{ size?: number }> = ({ size = 11 }) => (
  <Svg size={size} sw={2}>
    <path d="m5 12 7-7 7 7" />
    <path d="M12 19V5" />
  </Svg>
)

// The step-curve branch glyph in the real top bar's git cluster (a bottom-left
// dash + an S-curve rising to a flat top-right — see ref, right of the divider).
const BranchStepIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size} sw={2}>
    <path d="M3 17h5" />
    <path d="M7.5 17c4.5 0 2.5-10 7-10H21" />
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

const CircleUserIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size}>
    <circle cx="12" cy="12" r="10" />
    <circle cx="12" cy="10" r="3" />
    <path d="M7 20.662V19a2 2 0 0 1 2-2h6a2 2 0 0 1 2 2v1.662" />
  </Svg>
)

const ListTodoIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size}>
    <rect x="3" y="5" width="6" height="6" rx="1" />
    <path d="m3 17 2 2 4-4" />
    <path d="M13 6h8" />
    <path d="M13 12h8" />
    <path d="M13 18h8" />
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

const RocketIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size}>
    <path d="M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 0 0-2.91-.09z" />
    <path d="m12 15-3-3a22 22 0 0 1 2-3.95A12.88 12.88 0 0 1 22 2c0 2.72-.78 7.5-6 11a22.35 22.35 0 0 1-4 2z" />
    <path d="M9 12H4s.55-3.03 2-4c1.62-1.08 5 0 5 0" />
    <path d="M12 15v5s3.03-.55 4-2c1.08-1.62 0-5 0-5" />
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

// ── TopBar (38px, C.panel, hairline bottom border) ───────────────────────────
// Left: project glyph (dev = code, tinted project indigo; ref's dogfood board
// shows the feedback megaphone — pass glyph="megaphone" for a 1:1 ref match) +
// name 13/600 + muted globe + chevrons-up-down switcher.
// Right: run select + green play ▷ · divider · step-branch glyph + branch 13/600
// + commit check + muted ↑N context chip.
export type TopBarProps = {
  frame: number
  projectName?: string
  glyph?: "code" | "megaphone"
  showGlobe?: boolean
  runConfig?: string
  branch?: string
  ahead?: number // 0 hides the ↑N chip
}

export const TopBar: React.FC<TopBarProps> = ({
  projectName = IDENTITY.project,
  glyph = "code",
  showGlobe = true,
  runConfig = IDENTITY.runConfig,
  branch = IDENTITY.defaultBranch,
  ahead = 1,
}) => (
  <div
    style={{
      position: "absolute",
      top: 0,
      left: 0,
      right: 0,
      height: WIN.topBar,
      boxSizing: "border-box",
      backgroundColor: C.panel,
      borderBottom: `1px solid ${C.border}`,
      display: "flex",
      alignItems: "center",
      justifyContent: "space-between",
      padding: "0 12px",
      fontFamily: UI_FONT,
      zIndex: 20,
    }}
  >
    {/* left — project pill */}
    <div style={{ display: "flex", alignItems: "center" }}>
      <span style={{ color: C.indigoSoft, display: "flex" }}>
        {glyph === "code" ? <CodeIcon size={16} /> : <MegaphoneIcon size={16} />}
      </span>
      <span style={{ marginLeft: 8, fontSize: 13, fontWeight: 600, color: C.text, letterSpacing: -0.1 }}>{projectName}</span>
      {showGlobe ? (
        <span style={{ marginLeft: 8, color: C.muted, display: "flex" }}>
          <GlobeIcon size={13} />
        </span>
      ) : null}
      <span style={{ marginLeft: 7, color: C.muted, display: "flex" }}>
        <ChevronsUpDownIcon size={12} />
      </span>
    </div>

    {/* right — run bar · divider · git cluster */}
    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 5 }}>
        <span style={{ fontSize: 12, fontWeight: 500, color: C.text }}>{runConfig}</span>
        <span style={{ color: C.muted, display: "flex" }}>
          <ChevronDownIcon size={11} />
        </span>
      </div>
      <span style={{ color: C.green, display: "flex" }}>
        <PlayIcon size={13} />
      </span>
      <div style={{ width: 1, height: 16, backgroundColor: C.border, margin: "0 4px" }} />
      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
        <span style={{ color: C.text, display: "flex" }}>
          <BranchStepIcon size={14} />
        </span>
        <span style={{ fontSize: 13, fontWeight: 600, color: C.text, letterSpacing: -0.1 }}>{branch}</span>
      </div>
      <span style={{ color: C.muted, display: "flex" }}>
        <CheckIcon size={13} />
      </span>
      {ahead > 0 ? (
        <div style={{ display: "flex", alignItems: "center", gap: 2, color: C.muted }}>
          <ArrowUpIcon size={11} />
          <span style={{ fontSize: 12, fontWeight: 500 }}>{ahead}</span>
        </div>
      ) : null}
    </div>
  </div>
)

// ── IconRail (44px, C.panel, right hairline) ─────────────────────────────────
export type RailIconId =
  | "search"
  | "inbox"
  | "agents"
  | "issues"
  | "reviews"
  | "releases"
  | "files"
  | "source-control"
  | "settings"
  | "account"

// Window-local icon-button center Ys (rail spans y 38–980; pitch calibrated on
// the ref: one divider after search, tight ~24px rhythm, gear+account pinned low).
const RAIL_Y: Record<RailIconId, number> = {
  search: 57,
  inbox: 89,
  agents: 113,
  issues: 137,
  reviews: 161,
  releases: 185,
  files: 209,
  "source-control": 233,
  settings: 937,
  account: 961,
}
const RAIL_DIVIDER_Y = 73

const RAIL_ICON: Record<RailIconId, React.FC<{ size?: number }>> = {
  search: SearchIcon,
  inbox: InboxIcon,
  agents: CircleUserIcon,
  issues: ListTodoIcon,
  reviews: GitPullRequestIcon,
  releases: RocketIcon,
  files: FolderIcon,
  "source-control": GitMergeIcon,
  settings: SettingsIcon,
  account: CircleUserIcon,
}

const DEFAULT_RAIL_IDS: RailIconId[] = [
  "search",
  "inbox",
  "agents",
  "issues",
  "reviews",
  "releases",
  "files",
  "source-control",
  "settings",
  "account",
]

// Y map for an arbitrary icon list, following the ref rhythm: first icon at 57,
// divider, then a 24px pitch from 89; settings/account stay pinned low. For the
// default list this reproduces RAIL_Y exactly.
const railYMap = (ids: RailIconId[]): Record<string, number> => {
  const map: Record<string, number> = {}
  let slot = 0
  ids.forEach((id, i) => {
    if (id === "settings" || id === "account") {
      map[id] = RAIL_Y[id]
      return
    }
    if (i === 0) {
      map[id] = 57
      return
    }
    map[id] = 89 + 24 * slot
    slot += 1
  })
  return map
}

// Cursor-targeting helper: window-local center of a rail icon button.
export const railIconCenter = (id: string): { x: number; y: number } => ({
  x: WIN.rail / 2,
  y: RAIL_Y[id as RailIconId] ?? RAIL_Y.issues,
})

export type IconRailProps = {
  frame: number
  active: string
  // Slides the accent bar + crossfades icon tints from `from` to `active`,
  // starting at global frame `at` (10f, EASE). Resting state before `at` = `from`.
  activeTransition?: { from: string; at: number }
  dots?: string[] // rail ids that carry a small amber top-right dot
  dotColor?: string
  icons?: RailIconId[] // rail icon set (default: the full ships rail incl. releases)
}

export const IconRail: React.FC<IconRailProps> = ({ frame, active, activeTransition, dots = [], dotColor = C.synNumber, icons }) => {
  const ids = icons ?? DEFAULT_RAIL_IDS
  const yMap = icons === undefined ? (RAIL_Y as Record<string, number>) : railYMap(ids)
  const t = activeTransition
    ? interpolate(frame, [activeTransition.at, activeTransition.at + 10], [0, 1], { ...CLAMP, easing: EASE })
    : 1
  const fromId = activeTransition?.from
  // Accent bar center Y (slides between icons during a transition).
  const toY = yMap[active] ?? yMap.issues ?? RAIL_Y.issues
  const fromY = fromId !== undefined ? (yMap[fromId] ?? toY) : toY
  const barY = fromY + (toY - fromY) * t

  const tintOf = (id: RailIconId): string => {
    if (id === active && id === fromId) return C.indigoSoft
    if (id === active) return activeTransition ? interpolateColors(t, [0, 1], [C.muted, C.indigoSoft]) : C.indigoSoft
    if (id === fromId) return interpolateColors(t, [0, 1], [C.indigoSoft, C.muted])
    return C.muted
  }
  const bgOf = (id: RailIconId): number => {
    if (id === active && id === fromId) return 1
    if (id === active) return activeTransition ? t : 1
    if (id === fromId) return 1 - t
    return 0
  }

  return (
    <div
      style={{
        position: "absolute",
        top: WIN.topBar,
        left: 0,
        bottom: 0,
        width: WIN.rail,
        boxSizing: "border-box",
        backgroundColor: C.panel,
        borderRight: `1px solid ${C.borderSoft}`,
        zIndex: 10,
      }}
    >
      {/* divider after search */}
      <div style={{ position: "absolute", left: 12, top: RAIL_DIVIDER_Y - WIN.topBar, width: 20, height: 1, backgroundColor: C.border }} />
      {/* active accent bar (2px, rounded, hugs the rail's left edge) */}
      <div
        style={{
          position: "absolute",
          left: 2,
          top: barY - WIN.topBar - 9,
          width: 2,
          height: 18,
          borderRadius: 1,
          backgroundColor: C.indigoSoft,
        }}
      />
      {ids.map((id) => {
        const Icon = RAIL_ICON[id]
        return (
        <div
          key={id}
          style={{
            position: "absolute",
            left: WIN.rail / 2 - 11,
            top: (yMap[id] ?? RAIL_Y[id]) - WIN.topBar - 11,
            width: 22,
            height: 22,
            borderRadius: 5,
            backgroundColor: `rgba(255,255,255,${(0.05 * bgOf(id)).toFixed(3)})`,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: tintOf(id),
          }}
        >
          <Icon size={14} />
          {dots.includes(id) ? (
            <div
              style={{
                position: "absolute",
                top: 1,
                right: 1,
                width: 5,
                height: 5,
                borderRadius: 999,
                backgroundColor: dotColor,
              }}
            />
          ) : null}
        </div>
        )
      })}
    </div>
  )
}

// ── TabsBar (center tab strip, 29px, C.panel) ────────────────────────────────
export type ChromeTab = { id: string; label: string; mono?: boolean }

// Deterministic tab width so the assembler can aim the cursor (see tabsBarTabRect).
export const chromeTabWidth = (t: ChromeTab): number =>
  Math.min(240, Math.max(88, Math.round(12 + t.label.length * (t.mono ? 7.3 : 6.9) + 14 + 9 + 10)))

// Window-local rect of a tab (y 38–67). Returns null when the id isn't present.
export const tabsBarTabRect = (
  tabs: ChromeTab[],
  id: string,
  left = WIN.rail + WIN.sidebar,
): { x: number; y: number; w: number; h: number } | null => {
  let x = left
  for (const t of tabs) {
    const w = chromeTabWidth(t)
    if (t.id === id) return { x, y: WIN.topBar, w, h: 29 }
    x += w
  }
  return null
}

export type TabsBarProps = {
  frame: number
  tabs: ChromeTab[]
  activeId: string
  popAt?: Record<string, number> // tab id → global frame it POP-springs in (hidden before)
  left?: number // window-local x of the strip's left edge (default: center pane edge)
}

export const TabsBar: React.FC<TabsBarProps> = ({ frame, tabs, activeId, popAt, left = WIN.rail + WIN.sidebar }) => (
  <div
    style={{
      position: "absolute",
      top: WIN.topBar,
      left,
      right: 0,
      height: 29,
      boxSizing: "border-box",
      backgroundColor: C.panel,
      borderBottom: `1px solid ${C.borderSoft}`,
      display: "flex",
      alignItems: "stretch",
      overflow: "hidden",
      fontFamily: UI_FONT,
      zIndex: 15,
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
            position: "relative",
            width: chromeTabWidth(t),
            boxSizing: "border-box",
            display: "flex",
            alignItems: "center",
            padding: "0 10px 0 12px",
            backgroundColor: isActive ? C.accentBg : "transparent",
            scale: String(scale),
            opacity,
          }}
        >
          {isActive ? (
            <div style={{ position: "absolute", top: 0, left: 0, right: 0, height: 2, backgroundColor: C.indigoSoft }} />
          ) : null}
          <span
            style={{
              flex: 1,
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
          <span style={{ marginLeft: 14, color: isActive ? C.muted : C.dim, display: "flex" }}>
            <XIcon size={9} />
          </span>
        </div>
      )
    })}
  </div>
)

// ── DockCollapsedStrip (29px bottom strip: ▤ Terminal (1) … ⌃) ───────────────
export type DockCollapsedStripProps = {
  frame: number
  count?: number
}

export const DockCollapsedStrip: React.FC<DockCollapsedStripProps> = ({ count = 1 }) => (
  <div
    style={{
      position: "absolute",
      left: WIN.rail,
      right: 0,
      bottom: 0,
      height: WIN.dockStrip,
      boxSizing: "border-box",
      backgroundColor: C.panel,
      borderTop: `1px solid ${C.border}`,
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
    <span style={{ fontSize: 12, fontWeight: 500, color: C.muted }}>{`Terminal (${count})`}</span>
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

export const CenterEmptyState: React.FC<CenterEmptyStateProps> = ({ bottom = WIN.dockStrip, contentCenter }) => {
  const content = (
    <>
      <span style={{ color: C.dim, display: "flex" }}>
        <InboxIcon size={24} />
      </span>
      <div style={{ marginTop: 10, fontSize: 13, fontWeight: 500, color: C.text }}>Nothing open</div>
      <div style={{ marginTop: 4, fontSize: 12, color: C.muted }}>Pick an issue from the sidebar — it opens as a tab here.</div>
    </>
  )
  const paneLeft = WIN.rail + WIN.sidebar
  const paneTop = WIN.topBar + 29
  return (
    <div
      style={{
        position: "absolute",
        left: paneLeft,
        right: 0,
        top: paneTop,
        bottom,
        backgroundColor: C.bg,
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

// ── Cursor anchors (window-local approximations for the top bar's right cluster;
//    text is auto-laid-out, so treat these as ±4px) ───────────────────────────
export const CHROME_ANCHORS = {
  projectGlyph: { x: 20, y: 19 },
  projectSwitcher: { x: 146, y: 19 }, // chevrons-up-down
  runSelect: { x: 1376, y: 19 }, // "Dev Server ⌄"
  playButton: { x: 1430, y: 19 },
  branchChip: { x: 1480, y: 19 }, // "⎇ main"
  commitCheck: { x: 1521, y: 19 },
  aheadChip: { x: 1546, y: 19 }, // "↑1"
  dockStripChevron: { x: WIN.w - 17, y: WIN.h - WIN.dockStrip / 2 },
  dockStripLabel: { x: WIN.rail + 60, y: WIN.h - WIN.dockStrip / 2 },
} as const
