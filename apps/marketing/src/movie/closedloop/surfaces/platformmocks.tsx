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
type LineupRow = { id: string; title: string; priority: string; assignee?: string }
const boardRow = (id: string): LineupRow => {
  const row = CL_BOARD.find((r) => r.id === id)
  if (!row) throw new Error(`platforms: no board row ${id}`)
  return { id: row.id, title: row.title, priority: row.priority, assignee: row.assignee }
}
type LineupSection = { name: string; status: "in_progress" | "todo" | "done"; tint: string; count: number; rows: LineupRow[] }
export const LINEUP_SECTIONS: LineupSection[] = [
  {
    name: "In Progress",
    status: "in_progress",
    tint: C.tintInProgress,
    count: 2,
    rows: [boardRow("EXP-148"), boardRow("EXP-149")],
  },
  { name: "Todo", status: "todo", tint: C.tintTodo, count: 1, rows: [boardRow("EXP-150")] },
  {
    name: "Done",
    status: "done",
    tint: C.tintDone,
    count: 3,
    rows: [boardRow(NEW_ISSUE_ID), boardRow("EXP-144"), boardRow("EXP-147")],
  },
]

// ── Tiny UI glyphs shared by the web + phone mocks ───────────────────────────
const Glyph: React.FC<{ size: number; sw?: number; children: React.ReactNode }> = ({ size, sw = 1.9, children }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={sw} strokeLinecap="round" strokeLinejoin="round" style={{ display: "block", flexShrink: 0 }}>
    {children}
  </svg>
)

// Status icon per group (real-app vocabulary: hourglass / open circle / check).
const StatusGlyph: React.FC<{ status: LineupSection["status"]; size: number }> = ({ status, size }) => {
  if (status === "in_progress")
    return (
      <span style={{ color: C.statusInProgress, display: "flex" }}>
        <Glyph size={size} sw={2}>
          <path d="M6 3h12" />
          <path d="M6 21h12" />
          <path d="M7 3v4l5 5-5 5v4" />
          <path d="M17 3v4l-5 5 5 5v4" />
        </Glyph>
      </span>
    )
  if (status === "done")
    return (
      <span style={{ color: C.statusDone, display: "flex" }}>
        <svg width={size} height={size} viewBox="0 0 24 24" style={{ display: "block" }}>
          <circle cx="12" cy="12" r="10" fill="currentColor" />
          <path d="M8 12.5 11 15.5 16.5 9" fill="none" stroke={C.canvas} strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </span>
    )
  return (
    <span style={{ color: C.statusTodo, display: "flex" }}>
      <Glyph size={size} sw={2}>
        <circle cx="12" cy="12" r="9" />
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
export const WEB = { w: 560, chrome: 34, viewport: 348, sidebar: 148 } as const

const WebNavRow: React.FC<{ icon: React.ReactNode; label: string }> = ({ icon, label }) => (
  <div style={{ display: "flex", alignItems: "center", gap: 8, height: 26, padding: "0 10px", borderRadius: 6, color: C.muted }}>
    {icon}
    <span style={{ fontSize: 12 }}>{label}</span>
  </div>
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
      <div style={{ width: WEB.sidebar, flexShrink: 0, borderRight: `1px solid ${C.strokeCard}`, padding: "10px 6px", boxSizing: "border-box" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 7, padding: "0 10px", marginBottom: 10 }}>
          <span
            style={{
              width: 18,
              height: 18,
              borderRadius: 5,
              backgroundColor: C.indigoSoft,
              color: "#ffffff",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: 10.5,
              fontWeight: 700,
            }}
          >
            A
          </span>
          <span style={{ fontSize: 12, fontWeight: 600, color: C.text, whiteSpace: "nowrap" }}>{CL.project}</span>
          <span style={{ color: C.dim, display: "flex" }}>
            <Glyph size={9} sw={2.4}>
              <path d="m7 15 5 5 5-5" />
              <path d="m7 9 5-5 5 5" />
            </Glyph>
          </span>
        </div>
        <WebNavRow
          icon={
            <Glyph size={13}>
              <circle cx="11" cy="11" r="8" />
              <path d="m21 21-4.3-4.3" />
            </Glyph>
          }
          label="Search"
        />
        <WebNavRow
          icon={
            <Glyph size={13}>
              <path d="M22 12h-6l-2 3h-4l-2-3H2" />
              <path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />
            </Glyph>
          }
          label="Inbox"
        />
        <WebNavRow
          icon={
            <Glyph size={13}>
              <circle cx="18" cy="18" r="3" />
              <circle cx="6" cy="6" r="3" />
              <path d="M13 6h3a2 2 0 0 1 2 2v7" />
              <path d="M6 9v12" />
            </Glyph>
          }
          label="Reviews"
        />
        <WebNavRow
          icon={
            <Glyph size={13}>
              <circle cx="12" cy="12" r="10" />
              <circle cx="12" cy="12" r="4" />
              <path d="m4.9 4.9 4.2 4.2" />
              <path d="m14.9 14.9 4.2 4.2" />
              <path d="m14.9 9.1 4.2-4.2" />
              <path d="m4.9 19.1 4.2-4.2" />
            </Glyph>
          }
          label="Support"
        />
        <div style={{ margin: "12px 10px 5px", fontSize: 9.5, fontWeight: 600, letterSpacing: 0.6, textTransform: "uppercase", color: C.dim }}>Boards</div>
        <div style={{ display: "flex", alignItems: "center", gap: 8, height: 26, padding: "0 10px", borderRadius: 6, backgroundColor: C.fillActive }}>
          <span style={{ width: 8, height: 8, borderRadius: 3, backgroundColor: CL.projectColor, flexShrink: 0 }} />
          <span style={{ fontSize: 12, color: C.text }}>{CL.project}</span>
        </div>
      </div>

      {/* main — grouped issue list */}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: "flex", alignItems: "center", height: 36, padding: "0 12px", gap: 8 }}>
          <span style={{ fontSize: 13, fontWeight: 600, color: C.text }}>All Issues</span>
          <span style={{ marginLeft: "auto", fontSize: 11.5, color: C.muted }}>Filter</span>
          <span
            style={{
              height: 22,
              padding: "0 9px",
              borderRadius: 6,
              backgroundColor: C.indigo,
              color: "#ffffff",
              fontSize: 11,
              fontWeight: 600,
              display: "flex",
              alignItems: "center",
              gap: 4,
            }}
          >
            + New Issue
          </span>
        </div>
        {LINEUP_SECTIONS.map((section) => (
          <div key={section.name}>
            <div style={{ display: "flex", alignItems: "center", gap: 7, height: 25, padding: "0 12px", backgroundColor: section.tint }}>
              <StatusGlyph status={section.status} size={11} />
              <span style={{ fontSize: 11.5, fontWeight: 600, color: C.text }}>{section.name}</span>
              <span style={{ fontSize: 11, color: C.dim }}>{section.count}</span>
            </div>
            {section.rows.map((row) => (
              <div key={row.id} style={{ display: "flex", alignItems: "center", gap: 8, height: 30, padding: "0 12px", borderBottom: `1px solid ${C.strokeRow}` }}>
                <PriorityGlyph priority={row.priority} size={11} />
                <span style={{ fontFamily: MONO_FONT, fontSize: 10, color: C.dim, width: 48, flexShrink: 0 }}>{row.id}</span>
                <StatusGlyph status={section.status} size={11} />
                <span style={{ fontSize: 12, color: C.text, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{row.title}</span>
                {row.assignee ? (
                  <span style={{ marginLeft: "auto" }}>
                    <Avatar size={16} text={row.assignee} />
                  </span>
                ) : null}
              </div>
            ))}
          </div>
        ))}
      </div>
    </div>
  </div>
)
