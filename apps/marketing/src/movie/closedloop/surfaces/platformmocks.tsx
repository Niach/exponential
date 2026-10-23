// closedloop/surfaces/platformmocks.tsx — the platform-lineup hardware
// (EXP-385): the web app inside a browser window and the MacBook bezel (its
// screen is a frozen live IDE composition passed in by the segment), plus the
// per-client platform icon rows (marketing DownloadSection.tsx twins). The
// phone shot moved to the shared real-mobile surfaces (mobileui.tsx +
// steerphone's PhoneChassis, EXP-388). Recovered from the EXP-337-retired
// platforms.tsx (git 58fca05f^) and retinted onto the EXP-359 glass theme —
// the old C.bg/C.panel/C.border/C.accentBg tokens are gone.

import React from "react"
import {
  Bug,
  ChartLine,
  ChevronDown,
  ChevronsUpDown,
  Code,
  Inbox,
  ListTree,
  Megaphone,
  MessageCircle,
  Monitor,
  Package,
  Palette,
  Plus,
  Rocket,
  Search,
  Settings,
  Sparkles,
  SquareKanban,
  SquarePen,
  CalendarDays,
} from "lucide-react"
import { C, MONO_FONT, UI_FONT } from "../../ships/theme"
import {
  Avatar,
  PriorityIcon,
  StatusIcon,
} from "../../ships/surfaces/board"
import { CL, CL_BOARD, CL_BOARDS, NEW_ISSUE_ID } from "../fixtures"
import { ClaudeMark } from "./agentmarks"

// ── Platform icons (marketing DownloadSection.tsx twins) ─────────────────────
const FillIcon: React.FC<{ size: number; d: string }> = ({ size, d }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" style={{ display: "block" }}>
    <path d={d} />
  </svg>
)

export const AppleIcon: React.FC<{ size: number }> = ({ size }) => (
  <FillIcon
    size={size}
    d="M16.365 1.43c0 1.14-.417 2.2-1.11 2.98-.744.83-1.964 1.47-3.02 1.39-.13-1.1.42-2.26 1.06-2.99.72-.82 1.99-1.44 3.07-1.38zM20.5 17.02c-.55 1.28-.82 1.85-1.53 2.98-.99 1.58-2.39 3.55-4.12 3.56-1.54.02-1.94-1.01-4.03-1-2.09.01-2.53 1.02-4.07 1-1.73-.02-3.05-1.8-4.04-3.38C.02 16.72-.28 11.4 1.42 8.58c1.2-2 3.1-3.17 4.88-3.17 1.82 0 2.96 1.01 4.46 1.01 1.46 0 2.35-1.01 4.46-1.01 1.6 0 3.29.87 4.5 2.38-3.95 2.17-3.31 7.82.28 9.23z"
  />
)

export const WindowsIcon: React.FC<{ size: number }> = ({ size }) => (
  <FillIcon size={size} d="M3 5.55 10.62 4.5v6.98H3V5.55zm0 12.9 7.62 1.05v-6.9H3v5.85zM11.46 19.62 21.5 21V12.6H11.46v7.02zm0-15.24v7.1H21.5V3L11.46 4.38z" />
)

export const LinuxIcon: React.FC<{ size: number }> = ({ size }) => (
  <FillIcon
    size={size}
    d="M12 2c-2.1 0-3.4 1.7-3.4 3.9 0 1 .1 1.9.1 2.7 0 .7-.4 1.3-1 2.2-.9 1.3-2.1 2.9-2.9 4.6-.4.8-.6 1.5-.4 2.1.1.4.4.7.8.8-.1.4-.1.8 0 1.2.2.6.8 1 1.6 1.1.5.7 1.4 1.1 2.5 1.2h4.6c1.1-.1 2-.5 2.5-1.2.8-.1 1.4-.5 1.6-1.1.1-.4.1-.8 0-1.2.4-.1.7-.4.8-.8.2-.6 0-1.3-.4-2.1-.8-1.7-2-3.3-2.9-4.6-.6-.9-1-1.5-1-2.2 0-.8.1-1.7.1-2.7C15.4 3.7 14.1 2 12 2zm-1.5 4.1c.4 0 .7.4.7.9s-.3.9-.7.9-.7-.4-.7-.9.3-.9.7-.9zm3 0c.4 0 .7.4.7.9s-.3.9-.7.9-.7-.4-.7-.9.3-.9.7-.9zm-1.5 2.6c.7 0 1.6.5 1.6 1 0 .3-.4.5-.8.7-.3.2-.6.4-.8.4s-.5-.2-.8-.4c-.4-.2-.8-.4-.8-.7 0-.5.9-1 1.6-1z"
  />
)

// The Android robot head (marketing AndroidLogo twin).
export const AndroidIcon: React.FC<{ size: number }> = ({ size }) => (
  <FillIcon size={size} d="M17.6 9.48l1.84-3.18a.4.4 0 0 0-.7-.4l-1.87 3.23a11.4 11.4 0 0 0-9.74 0L5.26 5.9a.4.4 0 1 0-.7.4L6.4 9.48A10.8 10.8 0 0 0 1 18.2h22a10.8 10.8 0 0 0-5.4-8.72zM7 15.25a1 1 0 1 1 0-2 1 1 0 0 1 0 2zm10 0a1 1 0 1 1 0-2 1 1 0 0 1 0 2z" />
)

export const GlobeIcon: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" style={{ display: "block" }}>
    <circle cx="12" cy="12" r="9" />
    <path d="M3.2 12h17.6" />
    <path d="M12 3a13.4 13.4 0 0 1 0 18 13.4 13.4 0 0 1 0-18Z" />
  </svg>
)

// ── MacBook bezel (screen content is scaled from the full 1568×980 IDE) ──────
export const MAC_SCREEN = { w: 1568, h: 980 } as const

export const MacBook: React.FC<{ screenW: number; children: React.ReactNode }> = ({ screenW, children }) => {
  const scale = screenW / MAC_SCREEN.w
  const screenH = MAC_SCREEN.h * scale
  const bezel = 15
  const lidW = screenW + bezel * 2
  const deckW = Math.round(lidW * 1.12)
  return (
    <div style={{ width: deckW, display: "flex", flexDirection: "column", alignItems: "center" }}>
      {/* lid */}
      <div
        style={{
          position: "relative",
          width: lidW,
          padding: bezel,
          boxSizing: "border-box",
          borderRadius: 20,
          borderBottomLeftRadius: 6,
          borderBottomRightRadius: 6,
          backgroundColor: "#0b0b0d",
          border: "1px solid rgba(255,255,255,0.13)",
          boxShadow: "0 40px 110px rgba(0,0,0,0.6), 0 8px 26px rgba(0,0,0,0.4)",
        }}
      >
        {/* camera dot */}
        <div
          style={{
            position: "absolute",
            top: 5,
            left: "50%",
            width: 5,
            height: 5,
            marginLeft: -2.5,
            borderRadius: 999,
            backgroundColor: "#1f1f23",
          }}
        />
        <div style={{ position: "relative", width: screenW, height: screenH, borderRadius: 7, overflow: "hidden", backgroundColor: C.canvas }}>
          <div style={{ position: "absolute", left: 0, top: 0, width: MAC_SCREEN.w, height: MAC_SCREEN.h, transform: `scale(${scale})`, transformOrigin: "0 0" }}>
            {children}
          </div>
        </div>
      </div>
      {/* deck */}
      <div
        style={{
          position: "relative",
          width: deckW,
          height: 16,
          borderRadius: "0 0 14px 14px",
          background: "linear-gradient(#2e2e32, #191a1d)",
          border: "1px solid rgba(255,255,255,0.10)",
          borderTop: "none",
          boxShadow: "0 18px 44px rgba(0,0,0,0.5)",
        }}
      >
        {/* trackpad lip */}
        <div style={{ position: "absolute", top: 0, left: "50%", width: 150, height: 7, marginLeft: -75, borderRadius: "0 0 9px 9px", backgroundColor: "#101013" }} />
      </div>
    </div>
  )
}

// ── The web app inside a browser window ──────────────────────────────────────
// Copied from shots/board/web.webp (2026-10-23) and the code that draws it:
// apps/web components/team/sidebar.tsx (+ sidebar-agents / sidebar-running),
// packages/ui sidebar.tsx (compact rows h-7 text-sm), issue-list.tsx (the
// md grid: 1.25rem checkbox · 1.5rem priority · 4.5rem ID · 1.5rem status ·
// title · labels · 1.75rem assignee · 4.5rem due · 2rem), issue-group-band
// (py-1.5 pl-3 band, status wash /10) and the Pill primitive. The app is
// laid out at its REAL CSS metrics — the root font-size is 1.15625rem
// (18.5px) on desktop widths, packages/ui styles.css — in a 1600px-wide
// viewport, then scaled into the mock (EXP-1293 nav: Agent, Inbox, Threads,
// Devices; Agents; Apps; Boards; Active; Getting started + the user row).
// The nav's conditional Drafts row (while any) and the cloud-only Feedback
// row are absent. Post-story state: EXP-151 Done (merging never ends a
// session, so its thread stays live under Active), EXP-149 In Progress.
export const WEB = { w: 560, chrome: 34, viewport: 348, sidebar: 156 } as const

const CSS_W = 1600
const CSS_S = WEB.w / CSS_W
const CSS_H = WEB.viewport / CSS_S
const REM = 18.5
const rem = (n: number) => n * REM

// Sampled from the shot.
const WEBC = {
  ground: "#0c0b0e",
  panel: "#131215",
  panelStroke: "rgba(255,255,255,0.08)",
  fg: "#fafafa",
  muted: "#a1a1a1",
  groupLabel: "rgba(250,250,250,0.5)",
  active: "rgba(255,255,255,0.10)",
  primary: "#e5e5e5",
  primaryFg: "#171717",
  inboxDot: "#fafafa",
  live: "#22c55e",
  pillBg: "rgba(255,255,255,0.06)",
  pillStroke: "rgba(255,255,255,0.10)",
  bandRule: "rgba(255,255,255,0.04)",
} as const

const WASH: Record<"backlog" | "in_progress" | "done", string> = {
  backlog: "rgba(113,113,122,0.10)", // bg-zinc-500/10
  in_progress: "rgba(234,179,8,0.10)", // bg-yellow-500/10
  done: "rgba(59,130,246,0.10)", // bg-blue-500/10
}

const SIDEBAR_W = 316
const ICON = rem(1) // h-4 w-4

type WebStatus = "backlog" | "in_progress" | "done"
const WEB_GROUPS: { status: WebStatus; name: string; ids: string[] }[] = [
  { status: "backlog", name: "Backlog", ids: ["EXP-150", "EXP-145", "EXP-146"] },
  { status: "in_progress", name: "In Progress", ids: ["EXP-148", "EXP-149"] },
  { status: "done", name: "Done", ids: [NEW_ISSUE_ID, "EXP-144", "EXP-147"] },
]
const boardRow = (id: string) => {
  const row = CL_BOARD.find((r) => r.id === id)
  if (!row) throw new Error(`platforms: no board row ${id}`)
  return row
}

// The team's agents and apps (the shot's seed), drawn like the boards.
const WEB_AGENTS = [
  { name: "Triage", color: "#a855f7", Icon: Bug },
  { name: "Designer", color: "#ec4899", Icon: Palette },
  { name: "Release", color: "#22c55e", Icon: Package },
] as const
const WEB_APPS = [
  { name: "Devices", color: WEBC.fg, Icon: Monitor },
  { name: "Release checklist", color: "#f97316", Icon: Rocket },
  { name: "Team dashboard", color: "#0ea5e9", Icon: ChartLine },
] as const
const BOARD_ICON = { code: Code, kanban: SquareKanban, megaphone: Megaphone } as const

const Dot: React.FC<{ color: string }> = ({ color }) => (
  <span style={{ width: rem(0.5), height: rem(0.5), borderRadius: 999, backgroundColor: color, flexShrink: 0 }} />
)

// SidebarMenuButton density="compact": h-7, px-2, gap-2, text-sm, rounded-md.
const NavRow: React.FC<{
  icon: React.ReactNode
  label: string
  active?: boolean
  muted?: boolean
  trailing?: React.ReactNode
}> = ({ icon, label, active, muted, trailing }) => (
  <div
    style={{
      height: rem(1.75),
      display: "flex",
      alignItems: "center",
      gap: rem(0.5),
      padding: `0 ${rem(0.5)}px`,
      borderRadius: active ? 999 : rem(0.375),
      backgroundColor: active ? WEBC.active : "transparent",
      fontSize: rem(0.875),
      color: muted ? WEBC.muted : WEBC.fg,
    }}
  >
    <span style={{ display: "flex", flexShrink: 0 }}>{icon}</span>
    <span style={{ flex: 1, minWidth: 0, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{label}</span>
    {trailing}
  </div>
)

// SidebarGroup (p-2) with its label (h-8 text-xs /50) and the "+" action.
const Group: React.FC<{ label: string; action?: boolean; children: React.ReactNode }> = ({ label, action, children }) => (
  <div style={{ padding: rem(0.5) }}>
    <div
      style={{
        height: rem(2),
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: `0 ${rem(0.5)}px`,
        fontSize: rem(0.75),
        fontWeight: 500,
        letterSpacing: "0.025em",
        color: WEBC.groupLabel,
      }}
    >
      {label}
      {action ? <Plus size={ICON} color={WEBC.fg} strokeWidth={2} /> : null}
    </div>
    <div style={{ display: "flex", flexDirection: "column", gap: rem(0.25) }}>{children}</div>
  </div>
)

const WebSidebar: React.FC = () => (
  <div style={{ width: SIDEBAR_W, height: CSS_H, flexShrink: 0, display: "flex", flexDirection: "column" }}>
    {/* SidebarHeader p-2: team switcher · search · new issue */}
    <div style={{ padding: rem(0.5), display: "flex", alignItems: "center", gap: rem(0.25) }}>
      <div style={{ flex: 1, minWidth: 0, height: rem(2.5), display: "flex", alignItems: "center", gap: rem(0.5), padding: `0 ${rem(0.5)}px` }}>
        <span
          style={{
            width: 28,
            height: 28,
            borderRadius: 6,
            backgroundColor: "#e4e4e4",
            color: "#171717",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 13,
            fontWeight: 600,
            flexShrink: 0,
          }}
        >
          A
        </span>
        <span style={{ flex: 1, fontSize: rem(0.875), fontWeight: 600, color: WEBC.fg }}>{CL.team}</span>
        <ChevronsUpDown size={ICON} color={WEBC.fg} strokeWidth={2} />
      </div>
      <span style={{ width: rem(2), height: rem(2), display: "flex", alignItems: "center", justifyContent: "center", color: WEBC.muted }}>
        <Search size={ICON} strokeWidth={2} />
      </span>
      <span
        style={{
          width: rem(2),
          height: rem(2),
          borderRadius: 999,
          backgroundColor: WEBC.primary,
          color: WEBC.primaryFg,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <SquarePen size={ICON} strokeWidth={2} />
      </span>
    </div>
    <div style={{ height: 1, backgroundColor: "rgba(255,255,255,0.08)" }} />

    {/* SidebarContent: the groups, gap-2 */}
    <div style={{ flex: 1, minHeight: 0, overflow: "hidden", display: "flex", flexDirection: "column", gap: rem(0.5) }}>
      <div style={{ padding: rem(0.5), display: "flex", flexDirection: "column", gap: rem(0.25) }}>
        <NavRow icon={<MessageCircle size={ICON} strokeWidth={2} />} label="Agent" />
        <NavRow icon={<Inbox size={ICON} strokeWidth={2} />} label="Inbox" trailing={<Dot color={WEBC.inboxDot} />} />
        <NavRow icon={<ListTree size={ICON} strokeWidth={2} />} label="Threads" trailing={<Dot color={WEBC.live} />} />
        <NavRow icon={<Monitor size={ICON} strokeWidth={2} />} label="Devices" />
      </div>
      <Group label="Agents" action>
        {WEB_AGENTS.map(({ name, color, Icon }) => (
          <NavRow key={name} icon={<Icon size={ICON} color={color} strokeWidth={2} />} label={name} />
        ))}
      </Group>
      <Group label="Apps" action>
        {WEB_APPS.map(({ name, color, Icon }) => (
          <NavRow key={name} icon={<Icon size={ICON} color={color} strokeWidth={2} />} label={name} />
        ))}
      </Group>
      <Group label="Boards" action>
        {CL_BOARDS.map((b, i) => {
          const Icon = BOARD_ICON[b.glyph]
          return <NavRow key={b.name} active={i === 0} icon={<Icon size={ICON} color={b.color} strokeWidth={2} />} label={b.name} />
        })}
      </Group>
      <Group label="Active">
        <NavRow
          icon={<ClaudeMark size={rem(0.875)} />}
          label={`#${NEW_ISSUE_ID} ${boardRow(NEW_ISSUE_ID).title}`}
          trailing={<Monitor size={rem(0.875)} color={WEBC.muted} strokeWidth={2} />}
        />
      </Group>
    </div>

    {/* SidebarFooter p-2: Getting started, then the user row + settings */}
    <div style={{ padding: rem(0.5), display: "flex", flexDirection: "column", gap: rem(0.5) }}>
      <NavRow icon={<Sparkles size={ICON} strokeWidth={2} />} label="Getting started" muted />
      <div style={{ display: "flex", alignItems: "center", gap: rem(0.25) }}>
        <div style={{ flex: 1, height: rem(2), display: "flex", alignItems: "center", gap: rem(0.5), padding: `0 ${rem(0.5)}px` }}>
          <Avatar initials={CL.initials} size={28} />
          <span style={{ flex: 1, fontSize: rem(0.875), color: WEBC.fg }}>{CL.user.split(" ")[0]}</span>
          <ChevronsUpDown size={ICON} color={WEBC.fg} strokeWidth={2} />
        </div>
        <span style={{ width: rem(2), height: rem(2), display: "flex", alignItems: "center", justifyContent: "center", color: WEBC.fg }}>
          <Settings size={ICON} strokeWidth={2} />
        </span>
      </div>
    </div>
  </div>
)

// The board list (md grid) inside the cutout panel.
const GRID_RIGHT = rem(1.5) + rem(2) + rem(4.5) + rem(1.75)

const WebBoard: React.FC = () => (
  <div style={{ flex: 1, minHeight: 0, overflow: "hidden" }}>
    {WEB_GROUPS.map((g) => (
      <div key={g.status}>
        {/* IssueGroupBand: py-1.5 pl-3 pr-6, border-b border/40, wash */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: rem(0.375),
            height: 51,
            boxSizing: "border-box",
            padding: `0 ${rem(1.5)}px 0 ${rem(0.75)}px`,
            backgroundColor: WASH[g.status],
            borderBottom: `1px solid ${WEBC.bandRule}`,
          }}
        >
          <ChevronDown size={rem(0.75)} color={WEBC.muted} strokeWidth={2} />
          <StatusIcon status={g.status} size={rem(0.875)} />
          <span style={{ fontSize: rem(0.875), fontWeight: 500, color: WEBC.fg }}>{g.name}</span>
          <span style={{ fontSize: rem(0.75), color: WEBC.muted }}>{g.ids.length}</span>
        </div>
        {g.ids.map((id) => {
          const row = boardRow(id)
          return (
            <div
              key={id}
              style={{
                position: "relative",
                height: rem(2.5),
                display: "flex",
                alignItems: "center",
                padding: `0 ${rem(1.5)}px`,
              }}
            >
              <span style={{ width: rem(1.25), flexShrink: 0 }} />
              <span style={{ width: rem(1.5), flexShrink: 0, display: "flex", justifyContent: "center" }}>
                <PriorityIcon p={row.priority} size={rem(0.875)} />
              </span>
              <span style={{ width: rem(4.5), flexShrink: 0, fontFamily: MONO_FONT, fontSize: rem(0.75), color: WEBC.muted, paddingLeft: 6 }}>
                {row.id}
              </span>
              <span style={{ width: rem(1.5), flexShrink: 0, display: "flex", justifyContent: "center" }}>
                <StatusIcon status={g.status} size={rem(0.875)} />
              </span>
              <span
                style={{
                  flex: 1,
                  minWidth: 0,
                  marginLeft: rem(0.5),
                  fontSize: rem(0.875),
                  color: WEBC.fg,
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }}
              >
                {row.title}
              </span>
              {row.label ? (
                <span
                  style={{
                    marginLeft: rem(1),
                    height: rem(1.5),
                    boxSizing: "border-box",
                    display: "inline-flex",
                    alignItems: "center",
                    gap: rem(0.25),
                    padding: `0 ${rem(0.5)}px`,
                    borderRadius: 999,
                    border: `1px solid ${WEBC.pillStroke}`,
                    backgroundColor: WEBC.pillBg,
                    fontSize: rem(0.75),
                    fontWeight: 500,
                    color: "rgba(250,250,250,0.7)",
                    flexShrink: 0,
                  }}
                >
                  <span style={{ width: rem(0.375), height: rem(0.375), borderRadius: 999, backgroundColor: row.label.dot }} />
                  {row.label.name}
                </span>
              ) : null}
              <span style={{ width: GRID_RIGHT - rem(1.5), flexShrink: 0, display: "flex", alignItems: "center" }}>
                <span style={{ width: rem(1.75), display: "flex", justifyContent: "center" }}>
                  <Avatar initials={row.assignee} size={rem(1.25)} />
                </span>
                <span
                  style={{
                    width: rem(4.5),
                    display: "flex",
                    justifyContent: "flex-end",
                    alignItems: "center",
                    gap: rem(0.25),
                    fontSize: rem(0.75),
                    color: WEBC.muted,
                  }}
                >
                  {row.due ? (
                    <>
                      <CalendarDays size={rem(0.75)} strokeWidth={2} />
                      {row.due}
                    </>
                  ) : null}
                </span>
                <span style={{ width: rem(2) }} />
              </span>
            </div>
          )
        })}
      </div>
    ))}
  </div>
)

export const WebBrowserMock: React.FC = () => (
  <div
    style={{
      width: WEB.w,
      borderRadius: 12,
      overflow: "hidden",
      border: "1px solid rgba(255,255,255,0.13)",
      backgroundColor: WEBC.ground,
      boxShadow: "0 30px 80px rgba(0,0,0,0.55), 0 6px 20px rgba(0,0,0,0.4)",
      fontFamily: UI_FONT,
    }}
  >
    {/* browser chrome (not product UI) */}
    <div
      style={{
        position: "relative",
        height: WEB.chrome,
        backgroundColor: C.fillCard,
        borderBottom: `1px solid ${C.strokeCard}`,
        display: "flex",
        alignItems: "center",
      }}
    >
      <span style={{ display: "flex", gap: 6, paddingLeft: 12 }}>
        {["#f65f57", "#fbbc2e", "#28c840"].map((tone) => (
          <span key={tone} style={{ width: 10, height: 10, borderRadius: 999, backgroundColor: tone }} />
        ))}
      </span>
      <span
        style={{
          position: "absolute",
          left: "50%",
          translate: "-50% 0",
          height: 20,
          padding: "0 12px",
          borderRadius: 999,
          backgroundColor: C.fillActive,
          color: C.muted,
          fontSize: 10.5,
          display: "flex",
          alignItems: "center",
          gap: 5,
        }}
      >
        <svg width={9} height={9} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round" strokeLinejoin="round" style={{ display: "block" }}>
          <rect x="4" y="10" width="16" height="11" rx="2" />
          <path d="M8 10V7a4 4 0 0 1 8 0v3" />
        </svg>
        app.exponential.at
      </span>
    </div>

    {/* the web app at its real CSS metrics, scaled into the mock */}
    <div style={{ position: "relative", height: WEB.viewport, overflow: "hidden" }}>
      <div
        style={{
          position: "absolute",
          left: 0,
          top: 0,
          width: CSS_W,
          height: CSS_H,
          scale: String(CSS_S),
          transformOrigin: "0 0",
          display: "flex",
          backgroundColor: WEBC.ground,
          WebkitFontSmoothing: "antialiased",
        }}
      >
        <WebSidebar />
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
          {/* the work-tabs band: no tab open on a board page, just "+" */}
          <div style={{ height: 44, flexShrink: 0, display: "flex", alignItems: "center", paddingLeft: 15, color: WEBC.muted }}>
            <Plus size={14} strokeWidth={2} />
          </div>
          {/* the cutout panel */}
          <div
            style={{
              flex: 1,
              minHeight: 0,
              display: "flex",
              flexDirection: "column",
              margin: "0 10px 10px 10px",
              borderRadius: 16,
              border: `1px solid ${WEBC.panelStroke}`,
              backgroundColor: WEBC.panel,
              overflow: "hidden",
            }}
          >
            <WebBoard />
          </div>
        </div>
      </div>
    </div>
  </div>
)
