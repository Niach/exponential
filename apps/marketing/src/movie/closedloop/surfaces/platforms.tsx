// closedloop/surfaces/platforms.tsx — the S9 ending (EXP-200, reshaped by
// EXP-217): a full-frame opaque overlay. Phase one is the Shipped card (brand
// logo stroke-draw + display headline), which crossfades into the platform
// lineup — now ALL THREE clients under an Exponential logo + wordmark header:
// the web app inside a browser window (left), the IDE inside a MacBook
// (center — the frozen AppShell is passed in by Film.tsx; this module must
// not import it, Film imports us) and the mobile app inside an iPhone
// (right, a faithful recreation of the real mobile UI), each above its
// platform icon row (web · mac/windows/linux · iOS/Android). The overlay's
// backdrop stays opaque through the END_HOLD tail so the loop wraps from the
// bare ambient canvas, never from a half-faded app shot.
//
// Display type matches the marketing page: Geist resolves when the Player
// renders inside the marketing document (which self-hosts it); Remotion
// Studio falls back to Inter.

import React from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { C, EASE, MONO_FONT, UI_FONT } from "../../ships/theme"
import { ExpLogo } from "../../ships/rig"
import { CL, CL_BOARD, ENDING_COPY, NEW_ISSUE_ID } from "../fixtures"
import { ENDING } from "../timeline"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const EASED = { ...CLAMP, easing: EASE } as const

export const PAGE_FONT = `"Geist", "${UI_FONT}", ui-sans-serif, system-ui, sans-serif`

const rise = (frame: number, at: number, dur = 10, px = 14) => ({
  opacity: interpolate(frame, [at, at + dur], [0, 1], EASED),
  translate: `0px ${interpolate(frame, [at, at + dur], [px, 0], EASED)}px`,
})

// ── Platform icons (marketing DownloadSection.tsx twins) ─────────────────────
const FillIcon: React.FC<{ size: number; d: string }> = ({ size, d }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" style={{ display: "block" }}>
    <path d={d} />
  </svg>
)

const AppleIcon: React.FC<{ size: number }> = ({ size }) => (
  <FillIcon
    size={size}
    d="M16.365 1.43c0 1.14-.417 2.2-1.11 2.98-.744.83-1.964 1.47-3.02 1.39-.13-1.1.42-2.26 1.06-2.99.72-.82 1.99-1.44 3.07-1.38zM20.5 17.02c-.55 1.28-.82 1.85-1.53 2.98-.99 1.58-2.39 3.55-4.12 3.56-1.54.02-1.94-1.01-4.03-1-2.09.01-2.53 1.02-4.07 1-1.73-.02-3.05-1.8-4.04-3.38C.02 16.72-.28 11.4 1.42 8.58c1.2-2 3.1-3.17 4.88-3.17 1.82 0 2.96 1.01 4.46 1.01 1.46 0 2.35-1.01 4.46-1.01 1.6 0 3.29.87 4.5 2.38-3.95 2.17-3.31 7.82.28 9.23z"
  />
)

const WindowsIcon: React.FC<{ size: number }> = ({ size }) => (
  <FillIcon size={size} d="M3 5.55 10.62 4.5v6.98H3V5.55zm0 12.9 7.62 1.05v-6.9H3v5.85zM11.46 19.62 21.5 21V12.6H11.46v7.02zm0-15.24v7.1H21.5V3L11.46 4.38z" />
)

const LinuxIcon: React.FC<{ size: number }> = ({ size }) => (
  <FillIcon
    size={size}
    d="M12 2c-2.1 0-3.4 1.7-3.4 3.9 0 1 .1 1.9.1 2.7 0 .7-.4 1.3-1 2.2-.9 1.3-2.1 2.9-2.9 4.6-.4.8-.6 1.5-.4 2.1.1.4.4.7.8.8-.1.4-.1.8 0 1.2.2.6.8 1 1.6 1.1.5.7 1.4 1.1 2.5 1.2h4.6c1.1-.1 2-.5 2.5-1.2.8-.1 1.4-.5 1.6-1.1.1-.4.1-.8 0-1.2.4-.1.7-.4.8-.8.2-.6 0-1.3-.4-2.1-.8-1.7-2-3.3-2.9-4.6-.6-.9-1-1.5-1-2.2 0-.8.1-1.7.1-2.7C15.4 3.7 14.1 2 12 2zm-1.5 4.1c.4 0 .7.4.7.9s-.3.9-.7.9-.7-.4-.7-.9.3-.9.7-.9zm3 0c.4 0 .7.4.7.9s-.3.9-.7.9-.7-.4-.7-.9.3-.9.7-.9zm-1.5 2.6c.7 0 1.6.5 1.6 1 0 .3-.4.5-.8.7-.3.2-.6.4-.8.4s-.5-.2-.8-.4c-.4-.2-.8-.4-.8-.7 0-.5.9-1 1.6-1z"
  />
)

// The Android robot head (marketing AndroidLogo twin) — reads as "Android"
// where the old Play-Store triangle read as a storefront (EXP-217).
const AndroidIcon: React.FC<{ size: number }> = ({ size }) => (
  <FillIcon size={size} d="M17.6 9.48l1.84-3.18a.4.4 0 0 0-.7-.4l-1.87 3.23a11.4 11.4 0 0 0-9.74 0L5.26 5.9a.4.4 0 1 0-.7.4L6.4 9.48A10.8 10.8 0 0 0 1 18.2h22a10.8 10.8 0 0 0-5.4-8.72zM7 15.25a1 1 0 1 1 0-2 1 1 0 0 1 0 2zm10 0a1 1 0 1 1 0-2 1 1 0 0 1 0 2z" />
)

const GlobeIcon: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" style={{ display: "block" }}>
    <circle cx="12" cy="12" r="9" />
    <path d="M3.2 12h17.6" />
    <path d="M12 3a13.4 13.4 0 0 1 0 18 13.4 13.4 0 0 1 0-18Z" />
  </svg>
)

// ── The Shipped card (phase one) ──────────────────────────────────────────────
const ShippedCard: React.FC<{ frame: number }> = ({ frame }) => {
  const o =
    interpolate(frame, [ENDING.backdropIn, ENDING.backdropIn + 8], [0, 1], EASED) *
    (1 - interpolate(frame, [ENDING.cardOutFrom, ENDING.cardOutTo], [0, 1], EASED))
  if (o <= 0) return null
  const drawT = interpolate(frame, [ENDING.logoDrawFrom, ENDING.logoDrawTo], [0, 1], EASED)
  const discO = interpolate(frame, [ENDING.backdropIn + 2, ENDING.backdropIn + 12], [0, 1], EASED)
  return (
    <AbsoluteFill style={{ opacity: o, alignItems: "center", justifyContent: "center" }}>
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 44 }}>
        <div style={{ ...rise(frame, ENDING.backdropIn + 2, 12) }}>
          <ExpLogo size={148} drawT={drawT} discO={discO} />
        </div>
        <div
          style={{
            ...rise(frame, ENDING.titleAt),
            fontFamily: PAGE_FONT,
            fontSize: 132,
            fontWeight: 600,
            letterSpacing: "-0.04em",
            lineHeight: 1,
            color: C.text,
          }}
        >
          {ENDING_COPY.title}
        </div>
        <div
          style={{
            ...rise(frame, ENDING.subAt),
            marginTop: -14,
            fontFamily: PAGE_FONT,
            fontSize: 34,
            fontWeight: 500,
            letterSpacing: "-0.02em",
            color: C.muted,
          }}
        >
          {ENDING_COPY.sub}
        </div>
      </div>
    </AbsoluteFill>
  )
}

// ── MacBook bezel (no laptop chassis existed anywhere — built here) ──────────
const MAC_SCREEN = { w: 1568, h: 980 } as const

const MacBook: React.FC<{ screenW: number; children: React.ReactNode }> = ({ screenW, children }) => {
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
        <div style={{ position: "relative", width: screenW, height: screenH, borderRadius: 7, overflow: "hidden", backgroundColor: C.bg }}>
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

// ── Shared board fixture slices (EXP-151 landed in Done) ─────────────────────
type LineupRow = { id: string; title: string; priority: string; assignee?: string }
const boardRow = (id: string): LineupRow => {
  const row = CL_BOARD.find((r) => r.id === id)
  if (!row) throw new Error(`platforms: no board row ${id}`)
  return { id: row.id, title: row.title, priority: row.priority, assignee: row.assignee }
}
type LineupSection = { name: string; status: "in_progress" | "todo" | "done"; tint: string; count: number; rows: LineupRow[] }
const LINEUP_SECTIONS: LineupSection[] = [
  { name: "In Progress", status: "in_progress", tint: C.tintInProgress, count: 1, rows: [boardRow("EXP-148")] },
  { name: "Todo", status: "todo", tint: C.tintTodo, count: 2, rows: [boardRow("EXP-149"), boardRow("EXP-150")] },
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
          <path d="M8 12.5 11 15.5 16.5 9" fill="none" stroke={C.bg} strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round" />
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
      backgroundColor: C.accentBg,
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

// ── The web app inside a browser window (EXP-217 — the third client) ─────────
const WEB = { w: 560, chrome: 34, viewport: 348, sidebar: 148 } as const

const WebNavRow: React.FC<{ icon: React.ReactNode; label: string }> = ({ icon, label }) => (
  <div style={{ display: "flex", alignItems: "center", gap: 8, height: 26, padding: "0 10px", borderRadius: 6, color: C.muted }}>
    {icon}
    <span style={{ fontSize: 12 }}>{label}</span>
  </div>
)

const WebBrowserMock: React.FC = () => (
  <div
    style={{
      width: WEB.w,
      borderRadius: 12,
      overflow: "hidden",
      border: "1px solid rgba(255,255,255,0.13)",
      backgroundColor: C.bg,
      boxShadow: "0 30px 80px rgba(0,0,0,0.55), 0 6px 20px rgba(0,0,0,0.4)",
      fontFamily: UI_FONT,
    }}
  >
    {/* browser chrome */}
    <div
      style={{
        position: "relative",
        height: WEB.chrome,
        backgroundColor: C.panel,
        borderBottom: `1px solid ${C.border}`,
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
          backgroundColor: C.accentBg,
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
      <div style={{ width: WEB.sidebar, flexShrink: 0, borderRight: `1px solid ${C.border}`, padding: "10px 6px", boxSizing: "border-box" }}>
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
        <div style={{ display: "flex", alignItems: "center", gap: 8, height: 26, padding: "0 10px", borderRadius: 6, backgroundColor: C.accentBg }}>
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
              <div key={row.id} style={{ display: "flex", alignItems: "center", gap: 8, height: 30, padding: "0 12px", borderBottom: `1px solid ${C.borderRow}` }}>
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

// ── The phone (real mobile UI — EXP-217: team switcher header, filter chips,
//    card rows, floating pill tab bar + compose FAB) ──────────────────────────
const PHONE = { w: 330, screenW: 312, screenH: 660 } as const
const PHONE_TOTAL_H = PHONE.screenH + 18 // screen + bezel padding

const PhoneMock: React.FC = () => (
  <div
    style={{
      width: PHONE.w,
      padding: 9,
      boxSizing: "border-box",
      borderRadius: 44,
      backgroundColor: "#050505",
      border: "1px solid rgba(255,255,255,0.14)",
      boxShadow: "0 30px 70px rgba(0,0,0,0.55), 0 4px 18px rgba(0,0,0,0.4)",
      fontFamily: UI_FONT,
    }}
  >
    <div style={{ position: "relative", width: PHONE.screenW, height: PHONE.screenH, borderRadius: 36, overflow: "hidden", backgroundColor: C.bg }}>
      {/* status row */}
      <div style={{ position: "absolute", top: 12, left: 26, right: 26, display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <span style={{ fontSize: 14, fontWeight: 600, color: C.text, letterSpacing: 0.2 }}>9:41</span>
        <span style={{ display: "flex", alignItems: "center", gap: 5, color: C.text }}>
          <Glyph size={14}>
            <path d="M3 9.5C8.2 4.6 15.8 4.6 21 9.5" />
            <path d="M6.2 13.2c3.4-3.1 8.2-3.1 11.6 0" />
            <circle cx={12} cy={18} r={1.3} fill="currentColor" stroke="none" />
          </Glyph>
          <span style={{ width: 23, height: 12, borderRadius: 3.5, border: "1px solid rgba(255,255,255,0.5)", padding: 1.5, boxSizing: "border-box" }}>
            <span style={{ display: "block", width: "72%", height: "100%", borderRadius: 1.5, backgroundColor: C.text }} />
          </span>
        </span>
      </div>
      {/* dynamic island */}
      <div style={{ position: "absolute", top: 9, left: (PHONE.screenW - 88) / 2, width: 88, height: 25, borderRadius: 999, backgroundColor: "#000000", border: "1px solid rgba(255,255,255,0.05)" }} />

      {/* team switcher header (centered) + settings gear */}
      <div style={{ position: "absolute", top: 44, left: 0, right: 0, height: 34, display: "flex", alignItems: "center", justifyContent: "center", gap: 6 }}>
        <span style={{ fontSize: 15.5, fontWeight: 600, color: C.text }}>{CL.project}</span>
        <span style={{ color: C.muted, display: "flex" }}>
          <Glyph size={11} sw={2.4}>
            <path d="m7 15 5 5 5-5" />
            <path d="m7 9 5-5 5 5" />
          </Glyph>
        </span>
      </div>
      <span
        style={{
          position: "absolute",
          top: 44,
          right: 14,
          width: 30,
          height: 30,
          borderRadius: 999,
          backgroundColor: C.accentBg,
          color: C.muted,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Glyph size={14} sw={1.7}>
          <circle cx="12" cy="12" r="3" />
          <path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M18.7 5.3l-2.1 2.1M7.4 16.6l-2.1 2.1" />
        </Glyph>
      </span>

      {/* filter chips row */}
      <div style={{ position: "absolute", top: 88, left: 14, right: 14, display: "flex", alignItems: "center", gap: 7 }}>
        <span style={{ width: 28, height: 28, borderRadius: 999, backgroundColor: C.accentBg, color: C.muted, display: "flex", alignItems: "center", justifyContent: "center" }}>
          <Glyph size={13} sw={2}>
            <path d="M4 6h16" />
            <path d="M7 12h10" />
            <path d="M10 18h4" />
          </Glyph>
        </span>
        {[
          { label: "All issues", active: true },
          { label: "Active", active: false },
          { label: "Backlog", active: false },
        ].map(({ label, active }) => (
          <span
            key={label}
            style={{
              height: 28,
              padding: "0 13px",
              borderRadius: 999,
              backgroundColor: active ? C.primary : C.accentBg,
              color: active ? C.primaryFg : C.muted,
              fontSize: 12.5,
              fontWeight: active ? 600 : 500,
              display: "flex",
              alignItems: "center",
            }}
          >
            {label}
          </span>
        ))}
      </div>

      {/* grouped card list */}
      <div style={{ position: "absolute", top: 130, left: 0, right: 0 }}>
        {LINEUP_SECTIONS.map((section) => (
          <div key={section.name}>
            <div style={{ display: "flex", alignItems: "center", gap: 7, height: 30, padding: "0 16px" }}>
              <span style={{ color: C.dim, display: "flex" }}>
                <Glyph size={10} sw={2.4}>
                  <path d="m6 9 6 6 6-6" />
                </Glyph>
              </span>
              <StatusGlyph status={section.status} size={13} />
              <span style={{ fontSize: 13, fontWeight: 600, color: C.text }}>{section.name}</span>
              <span style={{ fontSize: 12, color: C.dim }}>{section.count}</span>
            </div>
            {section.rows.map((row) => (
              <div
                key={row.id}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 7,
                  height: 40,
                  margin: "0 12px 6px",
                  padding: "0 10px",
                  borderRadius: 12,
                  backgroundColor: C.panel,
                }}
              >
                <PriorityGlyph priority={row.priority} size={12} />
                <span style={{ fontFamily: MONO_FONT, fontSize: 10, color: C.dim, flexShrink: 0 }}>{row.id}</span>
                <StatusGlyph status={section.status} size={12} />
                <span style={{ fontSize: 12.5, color: C.text, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", flex: 1, minWidth: 0 }}>{row.title}</span>
                {row.assignee ? <Avatar size={20} text={row.assignee} /> : null}
                <span style={{ color: C.dim, display: "flex" }}>
                  <Glyph size={11} sw={2.2}>
                    <path d="m9 18 6-6-6-6" />
                  </Glyph>
                </span>
              </div>
            ))}
          </div>
        ))}
      </div>

      {/* floating pill tab bar + compose FAB */}
      <div
        style={{
          position: "absolute",
          left: 12,
          bottom: 14,
          height: 48,
          padding: "0 8px",
          borderRadius: 999,
          backgroundColor: "rgba(23,23,23,0.94)",
          border: `1px solid ${C.border}`,
          boxShadow: "0 10px 30px rgba(0,0,0,0.5)",
          display: "flex",
          alignItems: "center",
          gap: 2,
        }}
      >
        <span style={{ width: 34, height: 34, borderRadius: 999, backgroundColor: C.accentBg, color: C.text, display: "flex", alignItems: "center", justifyContent: "center" }}>
          <Glyph size={15} sw={2}>
            <path d="M8 6h13" />
            <path d="M8 12h13" />
            <path d="M8 18h13" />
            <path d="M3 6h.01" />
            <path d="M3 12h.01" />
            <path d="M3 18h.01" />
          </Glyph>
        </span>
        {[
          <Glyph key="inbox" size={15}>
            <path d="M22 12h-6l-2 3h-4l-2-3H2" />
            <path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />
          </Glyph>,
          <Glyph key="support" size={15}>
            <circle cx="12" cy="12" r="10" />
            <circle cx="12" cy="12" r="4" />
            <path d="m4.9 4.9 4.2 4.2" />
            <path d="m14.9 14.9 4.2 4.2" />
            <path d="m14.9 9.1 4.2-4.2" />
            <path d="m4.9 19.1 4.2-4.2" />
          </Glyph>,
          <Glyph key="agents" size={15}>
            <path d="M12 8V4H8" />
            <rect x="4" y="8" width="16" height="12" rx="2" />
            <path d="M2 14h2" />
            <path d="M20 14h2" />
            <path d="M15 13v2" />
            <path d="M9 13v2" />
          </Glyph>,
          <Glyph key="search" size={15}>
            <circle cx="11" cy="11" r="8" />
            <path d="m21 21-4.3-4.3" />
          </Glyph>,
        ].map((icon, i) => (
          <span key={i} style={{ width: 34, height: 34, borderRadius: 999, color: C.dim, display: "flex", alignItems: "center", justifyContent: "center" }}>
            {icon}
          </span>
        ))}
      </div>
      <span
        style={{
          position: "absolute",
          right: 12,
          bottom: 14,
          width: 48,
          height: 48,
          borderRadius: 999,
          backgroundColor: C.accentBg,
          border: `1px solid ${C.border}`,
          color: C.text,
          boxShadow: "0 10px 30px rgba(0,0,0,0.5)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Glyph size={17} sw={1.9}>
          <path d="M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
          <path d="M18.375 2.625a2.121 2.121 0 1 1 3 3L12 15l-4 1 1-4Z" />
        </Glyph>
      </span>
    </div>
  </div>
)

// ── The platform lineup (phase two) ──────────────────────────────────────────
const IconRow: React.FC<{ frame: number; at: number; icons: React.ReactNode[] }> = ({ frame, at, icons }) => (
  <div style={{ display: "flex", alignItems: "center", justifyContent: "center", gap: 36, color: "rgba(255,255,255,0.78)" }}>
    {icons.map((icon, i) => (
      <div key={i} style={{ ...rise(frame, at + i * 3, 9, 10) }}>
        {icon}
      </div>
    ))}
  </div>
)

const PHONE_SCALE = 0.78
const COLS = { web: 560, mac: 818, phone: Math.ceil(PHONE.w * PHONE_SCALE) } as const

const PlatformsScene: React.FC<{ frame: number; macScreen: React.ReactNode }> = ({ frame, macScreen }) => {
  const o = interpolate(frame, [ENDING.cardOutFrom + 6, ENDING.cardOutTo + 8], [0, 1], EASED)
  if (o <= 0) return null
  return (
    <AbsoluteFill style={{ opacity: o }}>
      {/* brand header — Exponential logo + wordmark (EXP-217) */}
      <div
        style={{
          position: "absolute",
          top: 72,
          left: 0,
          right: 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          gap: 18,
          ...rise(frame, ENDING.brandAt, 12, 16),
        }}
      >
        <ExpLogo size={46} />
        <span style={{ fontFamily: PAGE_FONT, fontSize: 40, fontWeight: 600, letterSpacing: "-0.03em", color: C.text }}>Exponential</span>
      </div>

      {/* the three clients, bottom-aligned on one shelf line */}
      <div
        style={{
          position: "absolute",
          top: 168,
          left: 0,
          right: 0,
          height: 660,
          display: "flex",
          alignItems: "flex-end",
          justifyContent: "center",
          gap: 56,
        }}
      >
        <div style={{ width: COLS.web, display: "flex", justifyContent: "center", ...rise(frame, ENDING.webAt, 12, 22) }}>
          <WebBrowserMock />
        </div>
        <div style={{ width: COLS.mac, display: "flex", justifyContent: "center", ...rise(frame, ENDING.macAt, 12, 22) }}>
          <MacBook screenW={700}>{macScreen}</MacBook>
        </div>
        <div style={{ width: COLS.phone, display: "flex", justifyContent: "center", ...rise(frame, ENDING.phoneAt, 12, 22) }}>
          <div style={{ width: PHONE.w * PHONE_SCALE, height: PHONE_TOTAL_H * PHONE_SCALE }}>
            <div style={{ transform: `scale(${PHONE_SCALE})`, transformOrigin: "0 0" }}>
              <PhoneMock />
            </div>
          </div>
        </div>
      </div>

      {/* per-client platform icon rows on one baseline */}
      <div style={{ position: "absolute", top: 856, left: 0, right: 0, display: "flex", justifyContent: "center", gap: 56 }}>
        <div style={{ width: COLS.web, ...rise(frame, ENDING.iconsAt, 10) }}>
          <IconRow frame={frame} at={ENDING.iconsAt} icons={[<GlobeIcon key="web" size={34} />]} />
        </div>
        <div style={{ width: COLS.mac, ...rise(frame, ENDING.iconsAt + 3, 10) }}>
          <IconRow
            frame={frame}
            at={ENDING.iconsAt + 3}
            icons={[<AppleIcon key="mac" size={34} />, <WindowsIcon key="win" size={32} />, <LinuxIcon key="linux" size={34} />]}
          />
        </div>
        <div style={{ width: COLS.phone, ...rise(frame, ENDING.iconsAt + 6, 10) }}>
          <IconRow frame={frame} at={ENDING.iconsAt + 6} icons={[<AppleIcon key="ios" size={32} />, <AndroidIcon key="android" size={34} />]} />
        </div>
      </div>
    </AbsoluteFill>
  )
}

// ── The full-frame ending overlay ────────────────────────────────────────────
// Mounted by Film.tsx above the camera layer. `macScreen` is the frozen
// AppShell (Film owns it — importing it here would be circular). The backdrop
// repeats the composition's ambient gradient so the END_HOLD rest tail and
// the loop wrap live on the same canvas the film opens on.
export const EndingOverlay: React.FC<{ frame: number; macScreen: React.ReactNode }> = ({ frame, macScreen }) => {
  if (frame < ENDING.backdropIn) return null
  const backdropO = interpolate(frame, [ENDING.backdropIn, ENDING.backdropIn + 8], [0, 1], EASED)
  const contentO = 1 - interpolate(frame, [ENDING.fadeOutFrom, ENDING.fadeOutTo], [0, 1], EASED)
  return (
    <AbsoluteFill style={{ opacity: backdropO }}>
      <AbsoluteFill style={{ backgroundColor: C.canvas }}>
        <AbsoluteFill style={{ background: `radial-gradient(720px 520px at 50% 32%, rgba(99,102,241,0.20), transparent 70%)` }} />
        <AbsoluteFill style={{ background: `radial-gradient(600px 400px at 88% 92%, rgba(129,140,248,0.10), transparent 70%)` }} />
      </AbsoluteFill>
      <AbsoluteFill style={{ opacity: contentO }}>
        <ShippedCard frame={frame} />
        {frame >= ENDING.cardOutFrom ? <PlatformsScene frame={frame} macScreen={macScreen} /> : null}
      </AbsoluteFill>
    </AbsoluteFill>
  )
}
