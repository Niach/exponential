// closedloop/surfaces/platformmocks.tsx — the platform-lineup hardware
// (EXP-385): the web app inside a browser window and the MacBook bezel (its
// screen is a frozen live IDE composition passed in by the segment), plus the
// per-client platform icon rows (marketing DownloadSection.tsx twins). The
// phone shot moved to the shared real-mobile surfaces (mobileui.tsx +
// steerphone's PhoneChassis, EXP-388). Recovered from the EXP-337-retired
// platforms.tsx (git 58fca05f^) and retinted onto the EXP-359 glass theme —
// the old C.bg/C.panel/C.border/C.accentBg tokens are gone.

import React from "react"
import { C, MONO_FONT, UI_FONT } from "../../ships/theme"
import { CL, CL_BOARD, NEW_ISSUE_ID } from "../fixtures"

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

// ── Shared board fixture slices (post-story: EXP-151 shipped, EXP-149 live) ──
type LineupRow = {
  id: string
  title: string
  priority: string
  assignee?: string
  label?: { name: string; dot: string }
  due?: string
}
const boardRow = (id: string): LineupRow => {
  const row = CL_BOARD.find((r) => r.id === id)
  if (!row) throw new Error(`platforms: no board row ${id}`)
  return {
    id: row.id,
    title: row.title,
    priority: row.priority,
    assignee: row.assignee,
    label: row.label,
    due: row.due,
  }
}
type LineupSection = {
  name: string
  status: "backlog" | "in_progress" | "done"
  tint: string
  count: number
  rows: LineupRow[]
}
// Contract displayOrder — the same grouping the desktop and mobile lists use.
export const LINEUP_SECTIONS: LineupSection[] = [
  {
    name: "Backlog",
    status: "backlog",
    tint: C.tintBacklog,
    count: 3,
    rows: [boardRow("EXP-150"), boardRow("EXP-146")],
  },
  {
    name: "In Progress",
    status: "in_progress",
    tint: C.tintInProgress,
    count: 2,
    rows: [boardRow("EXP-148"), boardRow("EXP-149")],
  },
  {
    name: "Done",
    status: "done",
    tint: C.tintDone,
    count: 3,
    rows: [boardRow(NEW_ISSUE_ID), boardRow("EXP-144")],
  },
]

// ── Tiny UI glyphs shared by the web + phone mocks ───────────────────────────
const Glyph: React.FC<{ size: number; sw?: number; children: React.ReactNode }> = ({ size, sw = 1.9, children }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={sw} strokeLinecap="round" strokeLinejoin="round" style={{ display: "block", flexShrink: 0 }}>
    {children}
  </svg>
)

// Status icon per group — icons.json status-*: circle-dashed / circle /
// progress-2-4 (the pie clock) / circle-check.
const StatusGlyph: React.FC<{
  status: LineupSection["status"]
  size: number
}> = ({ status, size }) => {
  if (status === "backlog")
    return (
      <span style={{ color: C.statusBacklog, display: "flex" }}>
        <Glyph size={size} sw={2}>
          <circle cx="12" cy="12" r="9" strokeDasharray="3.6 3.4" />
        </Glyph>
      </span>
    )
  if (status === "in_progress")
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
          <path
            d="M12 12 L12 6 A6 6 0 0 1 12 18 Z"
            fill="currentColor"
            stroke="none"
          />
        </svg>
      </span>
    )
  return (
    <span style={{ color: C.statusDone, display: "flex" }}>
      <Glyph size={size} sw={2}>
        <circle cx="12" cy="12" r="9" />
        <path d="m8.5 12 2.5 2.5 5-5" />
      </Glyph>
    </span>
  )
}

// Priority signal bars (high 3 · medium 2 · low 1 · none —).
const PriorityGlyph: React.FC<{ priority: string; size: number }> = ({ priority, size }) => {
  if (priority === "none" || priority === "")
    return (
      <svg width={size} height={size} viewBox="0 0 24 24" style={{ display: "block", flexShrink: 0 }}>
        <rect x="5" y="11" width="14" height="2.6" rx="1.3" fill={C.dim} />
      </svg>
    )
  const lit = priority === "high" ? 3 : priority === "medium" ? 2 : 1
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" style={{ display: "block", flexShrink: 0 }}>
      {[0, 1, 2].map((i) => (
        <rect
          key={i}
          x={5 + i * 5.4}
          y={15 - i * 4.5}
          width={3.4}
          height={5 + i * 4.5}
          rx={1.4}
          fill={i < lit ? C.muted : "rgba(255,255,255,0.16)"}
        />
      ))}
    </svg>
  )
}

const Avatar: React.FC<{ size: number; text: string }> = ({ size, text }) => (
  <span
    style={{
      width: size,
      height: size,
      borderRadius: 999,
      backgroundColor: C.fillActive,
      color: C.text,
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      fontSize: size * 0.44,
      fontWeight: 600,
      flexShrink: 0,
    }}
  >
    {text}
  </span>
)

// ── The web app inside a browser window ──────────────────────────────────────
// EXP-471: matched to shots/board/web.webp — the sidebar opens with the team
// switcher plus the round search + compose buttons, the nav is Inbox / Reviews
// / Agents / Support with count badges, a "Boards" group carries the colored
// board glyphs, and Getting started + the user row are pinned at the bottom.
// The list header holds ONLY the ghost Filter button, and the agent dock bar
// rides along the viewport's bottom edge.
export const WEB = { w: 560, chrome: 34, viewport: 348, sidebar: 156 } as const

const WEB_NAV: { label: string; badge?: string; icon: React.ReactNode }[] = [
  {
    label: "Inbox",
    badge: "2",
    icon: (
      <Glyph size={13}>
        <path d="M22 12h-6l-2 3h-4l-2-3H2" />
        <path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />
      </Glyph>
    ),
  },
  {
    label: "Reviews",
    badge: "1",
    icon: (
      <Glyph size={13}>
        <circle cx="18" cy="18" r="3" />
        <circle cx="6" cy="6" r="3" />
        <path d="M13 6h3a2 2 0 0 1 2 2v7" />
        <path d="M6 9v12" />
      </Glyph>
    ),
  },
  {
    label: "Agents",
    icon: (
      <Glyph size={13} sw={1.7}>
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
    label: "Support",
    icon: (
      <Glyph size={13}>
        <circle cx="12" cy="12" r="10" />
        <circle cx="12" cy="12" r="4" />
        <path d="m4.9 4.9 4.2 4.2" />
        <path d="m14.9 14.9 4.2 4.2" />
        <path d="m14.9 9.1 4.2-4.2" />
        <path d="m4.9 19.1 4.2-4.2" />
      </Glyph>
    ),
  },
]

const WebNavRow: React.FC<{
  icon: React.ReactNode
  label: string
  badge?: string
}> = ({ icon, label, badge }) => (
  <div
    style={{
      display: "flex",
      alignItems: "center",
      gap: 9,
      height: 25,
      padding: "0 9px",
      borderRadius: 7,
      color: C.muted,
    }}
  >
    {icon}
    <span style={{ flex: 1, fontSize: 12 }}>{label}</span>
    {badge ? (
      <span style={{ fontSize: 10.5, color: C.dim }}>{badge}</span>
    ) : null}
  </div>
)

const WEB_BOARDS: { name: string; color: string; glyph: React.ReactNode }[] = [
  {
    name: CL.project,
    color: "#818cf8",
    glyph: (
      <Glyph size={12} sw={2.2}>
        <path d="m16 18 6-6-6-6" />
        <path d="m8 6-6 6 6 6" />
      </Glyph>
    ),
  },
  {
    name: "Launch Marketing",
    color: "#f59e0b",
    glyph: (
      <Glyph size={12} sw={2}>
        <rect x="3" y="3" width="18" height="18" rx="2" />
        <path d="M8 7v7" />
        <path d="M12 7v4" />
        <path d="M16 7v9" />
      </Glyph>
    ),
  },
  {
    name: "Product Feedback",
    color: "#22c55e",
    glyph: (
      <Glyph size={12} sw={2}>
        <path d="m3 11 18-5v12L3 14v-3z" />
        <path d="M11.6 16.8a3 3 0 1 1-5.8-1.6" />
      </Glyph>
    ),
  },
]

const RoundButton: React.FC<{ children: React.ReactNode; solid?: boolean }> = ({
  children,
  solid,
}) => (
  <span
    style={{
      width: 20,
      height: 20,
      flexShrink: 0,
      borderRadius: 999,
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      backgroundColor: solid ? "rgba(255,255,255,0.14)" : "transparent",
      border: solid ? "1px solid rgba(255,255,255,0.14)" : "none",
      color: solid ? C.text : C.muted,
    }}
  >
    {children}
  </span>
)

export const WebBrowserMock: React.FC = () => (
  <div
    style={{
      width: WEB.w,
      borderRadius: 12,
      overflow: "hidden",
      border: "1px solid rgba(255,255,255,0.13)",
      backgroundColor: C.canvas,
      boxShadow: "0 30px 80px rgba(0,0,0,0.55), 0 6px 20px rgba(0,0,0,0.4)",
      fontFamily: UI_FONT,
    }}
  >
    {/* browser chrome */}
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
          <span
            key={tone}
            style={{
              width: 10,
              height: 10,
              borderRadius: 999,
              backgroundColor: tone,
            }}
          />
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
        <Glyph size={9} sw={2.2}>
          <rect x="4" y="10" width="16" height="11" rx="2" />
          <path d="M8 10V7a4 4 0 0 1 8 0v3" />
        </Glyph>
        app.exponential.at
      </span>
    </div>

    {/* the web app */}
    <div style={{ display: "flex", height: WEB.viewport }}>
      {/* sidebar */}
      <div
        style={{
          width: WEB.sidebar,
          flexShrink: 0,
          borderRight: `1px solid ${C.strokeCard}`,
          padding: "9px 6px 8px",
          boxSizing: "border-box",
          display: "flex",
          flexDirection: "column",
        }}
      >
        {/* team switcher + search + compose */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            padding: "0 6px",
            marginBottom: 9,
          }}
        >
          <span
            style={{
              width: 18,
              height: 18,
              flexShrink: 0,
              borderRadius: 6,
              backgroundColor: "#ededed",
              color: "#18181b",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: 10.5,
              fontWeight: 700,
            }}
          >
            A
          </span>
          <span
            style={{
              flex: 1,
              minWidth: 0,
              fontSize: 12,
              fontWeight: 600,
              color: C.text,
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
            }}
          >
            Acme
          </span>
          <span style={{ color: C.dim, display: "flex", flexShrink: 0 }}>
            <Glyph size={9} sw={2.4}>
              <path d="m7 15 5 5 5-5" />
              <path d="m7 9 5-5 5 5" />
            </Glyph>
          </span>
          <RoundButton>
            <Glyph size={12}>
              <circle cx="11" cy="11" r="8" />
              <path d="m21 21-4.3-4.3" />
            </Glyph>
          </RoundButton>
          <RoundButton solid>
            <Glyph size={11} sw={2}>
              <path d="M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
              <path d="M18.4 2.6a2 2 0 0 1 3 3L12 15l-4 1 1-4Z" />
            </Glyph>
          </RoundButton>
        </div>
        {WEB_NAV.map((row) => (
          <WebNavRow
            key={row.label}
            icon={row.icon}
            label={row.label}
            badge={row.badge}
          />
        ))}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            margin: "10px 9px 4px",
            color: C.dim,
          }}
        >
          <span style={{ flex: 1, fontSize: 10.5, fontWeight: 500 }}>
            Boards
          </span>
          <Glyph size={10} sw={2.2}>
            <path d="M5 12h14" />
            <path d="M12 5v14" />
          </Glyph>
        </div>
        {WEB_BOARDS.map((b, i) => (
          <div
            key={b.name}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 9,
              height: 25,
              padding: "0 9px",
              borderRadius: 7,
              backgroundColor: i === 0 ? C.fillActive : "transparent",
            }}
          >
            <span style={{ color: b.color, display: "flex" }}>{b.glyph}</span>
            <span
              style={{
                fontSize: 12,
                color: i === 0 ? C.text : C.muted,
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
            >
              {b.name}
            </span>
          </div>
        ))}
        <div style={{ flex: 1 }} />
        <WebNavRow
          icon={
            <Glyph size={13} sw={1.7}>
              <path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z" />
              <path d="M20 3v4" />
              <path d="M22 5h-4" />
            </Glyph>
          }
          label="Getting started"
        />
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            height: 25,
            padding: "0 9px",
            color: C.muted,
          }}
        >
          <Avatar size={17} text={CL.initials} />
          <span
            style={{ flex: 1, fontSize: 12, fontWeight: 500, color: C.text }}
          >
            {CL.user.split(" ")[0]}
          </span>
          <Glyph size={11} sw={1.7}>
            <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
            <circle cx="12" cy="12" r="3" />
          </Glyph>
        </div>
      </div>

      {/* main — grouped issue list + the agent dock bar */}
      <div
        style={{
          flex: 1,
          minWidth: 0,
          display: "flex",
          flexDirection: "column",
        }}
      >
        <div
          style={{
            flex: "none",
            display: "flex",
            alignItems: "center",
            justifyContent: "flex-end",
            height: 34,
            padding: "0 12px",
            gap: 5,
            color: C.muted,
            fontSize: 11.5,
          }}
        >
          <Glyph size={11} sw={1.8}>
            <path d="M3 6h18" />
            <path d="M7 12h10" />
            <path d="M11 18h4" />
          </Glyph>
          Filter
        </div>
        <div style={{ flex: 1, minHeight: 0, overflow: "hidden" }}>
          {LINEUP_SECTIONS.map((section) => (
            <div key={section.name}>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  height: 24,
                  padding: "0 11px",
                  backgroundColor: section.tint,
                }}
              >
                <span style={{ color: C.dim, display: "flex" }}>
                  <Glyph size={9} sw={2.4}>
                    <path d="m6 9 6 6 6-6" />
                  </Glyph>
                </span>
                <StatusGlyph status={section.status} size={11} />
                <span
                  style={{ fontSize: 11.5, fontWeight: 600, color: C.text }}
                >
                  {section.name}
                </span>
                <span style={{ fontSize: 10.5, color: C.muted }}>
                  {section.count}
                </span>
              </div>
              {section.rows.map((row) => (
                <div
                  key={row.id}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 7,
                    height: 28,
                    padding: "0 11px 0 20px",
                    borderBottom: `1px solid ${C.strokeRow}`,
                  }}
                >
                  <PriorityGlyph priority={row.priority} size={11} />
                  <span
                    style={{
                      fontFamily: MONO_FONT,
                      fontSize: 9.5,
                      color: C.dim,
                      width: 46,
                      flexShrink: 0,
                    }}
                  >
                    {row.id}
                  </span>
                  <StatusGlyph status={section.status} size={11} />
                  <span
                    style={{
                      flex: 1,
                      minWidth: 0,
                      fontSize: 11.5,
                      color: C.text,
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
                        display: "flex",
                        alignItems: "center",
                        gap: 4,
                        flexShrink: 0,
                        fontSize: 10.5,
                        color: C.muted,
                      }}
                    >
                      <span
                        style={{
                          width: 5,
                          height: 5,
                          borderRadius: 999,
                          backgroundColor: row.label.dot,
                        }}
                      />
                      {row.label.name.charAt(0).toUpperCase() +
                        row.label.name.slice(1)}
                    </span>
                  ) : null}
                  {row.assignee ? <Avatar size={15} text={row.assignee} /> : null}
                </div>
              ))}
            </div>
          ))}
        </div>
        {/* agent dock bar — the live sessions strip */}
        <div
          style={{
            flex: "none",
            height: 24,
            display: "flex",
            alignItems: "center",
            gap: 6,
            padding: "0 10px",
            borderTop: `1px solid ${C.strokeRow}`,
          }}
        >
          {[NEW_ISSUE_ID, "EXP-149"].map((id) => (
            <span
              key={id}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 5,
                height: 16,
                padding: "0 7px",
                borderRadius: 999,
                backgroundColor: C.fillRow,
                fontSize: 9.5,
                color: C.muted,
                whiteSpace: "nowrap",
              }}
            >
              <span
                style={{
                  width: 5,
                  height: 5,
                  borderRadius: 999,
                  backgroundColor: C.green,
                }}
              />
              <span style={{ color: C.text, fontWeight: 600 }}>{id}</span>
              {`· ${CL.user.split(" ")[0]}'s MacBook Pro`}
            </span>
          ))}
        </div>
      </div>
    </div>
  </div>
)
