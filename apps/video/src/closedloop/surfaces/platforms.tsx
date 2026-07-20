// closedloop/surfaces/platforms.tsx — the S9 ending (EXP-200): a full-frame
// opaque overlay that replaces the old reply-email beat. Phase one is the
// Shipped card (brand logo stroke-draw + display headline), which crossfades
// into the platform lineup: the IDE inside a MacBook on the left (the frozen
// AppShell is passed in by Film.tsx — this module must not import it, Film
// imports us) and the mobile app inside an iPhone on the right, each with its
// platform icon row (web/mac/windows/linux · App Store/Play Store). The
// overlay's backdrop stays opaque through the END_HOLD tail so the loop wraps
// from the bare ambient canvas, never from a half-faded app shot.
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

// ── Platform icons (marketing DownloadSection.tsx twins + globe/Play Store) ───
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

// Stroke-based App Store badge — the marketing SVG's letter subpaths don't
// punch out of the disc under nonzero fill, which reads as a solid blob at
// icon size on the dark canvas.
const AppStoreIcon: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round" style={{ display: "block" }}>
    <circle cx="12" cy="12" r="9" />
    <path d="M8.4 15.4 12 8.6l3.6 6.8" />
    <path d="M9.9 12.9h4.2" />
  </svg>
)

const PlayStoreIcon: React.FC<{ size: number }> = ({ size }) => (
  <FillIcon size={size} d="M4.5 3.6v16.8c0 .62.67 1.01 1.2.7l14.66-8.4a.8.8 0 0 0 0-1.4L5.7 2.9a.8.8 0 0 0-1.2.7z" />
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

// ── The phone (mobile app board — EXP-151 landed in Done) ────────────────────
const PHONE = { w: 330, screenW: 312, screenH: 660 } as const

type PhoneRow = { id: string; title: string }
const boardRow = (id: string): PhoneRow => {
  const row = CL_BOARD.find((r) => r.id === id)
  if (!row) throw new Error(`platforms: no board row ${id}`)
  return { id: row.id, title: row.title }
}
const PHONE_SECTIONS: { name: string; tint: string; dot: string; count: number; rows: PhoneRow[] }[] = [
  { name: "In Progress", tint: C.tintInProgress, dot: C.statusInProgress, count: 1, rows: [boardRow("EXP-148")] },
  { name: "Todo", tint: C.tintTodo, dot: C.statusTodo, count: 2, rows: [boardRow("EXP-149"), boardRow("EXP-150")] },
  {
    name: "Done",
    tint: C.tintDone,
    dot: C.statusDone,
    count: 3,
    rows: [boardRow(NEW_ISSUE_ID), boardRow("EXP-144"), boardRow("EXP-147")],
  },
]

const PhoneGlyph: React.FC<{ size: number; children: React.ReactNode }> = ({ size, children }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.9} strokeLinecap="round" strokeLinejoin="round" style={{ display: "block" }}>
    {children}
  </svg>
)

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
          <PhoneGlyph size={14}>
            <path d="M3 9.5C8.2 4.6 15.8 4.6 21 9.5" />
            <path d="M6.2 13.2c3.4-3.1 8.2-3.1 11.6 0" />
            <circle cx={12} cy={18} r={1.3} fill="currentColor" stroke="none" />
          </PhoneGlyph>
          <span style={{ width: 23, height: 12, borderRadius: 3.5, border: "1px solid rgba(255,255,255,0.5)", padding: 1.5, boxSizing: "border-box" }}>
            <span style={{ display: "block", width: "72%", height: "100%", borderRadius: 1.5, backgroundColor: C.text }} />
          </span>
        </span>
      </div>
      {/* dynamic island */}
      <div style={{ position: "absolute", top: 9, left: (PHONE.screenW - 88) / 2, width: 88, height: 25, borderRadius: 999, backgroundColor: "#000000", border: "1px solid rgba(255,255,255,0.05)" }} />
      {/* team header */}
      <div style={{ position: "absolute", top: 52, left: 18, right: 18 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 7 }}>
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
          <span style={{ fontSize: 12.5, color: C.muted }}>{CL.project}</span>
        </div>
        <div style={{ marginTop: 8, fontSize: 21, fontWeight: 700, letterSpacing: -0.4, color: C.text }}>All Issues</div>
      </div>
      {/* board groups */}
      <div style={{ position: "absolute", top: 118, left: 0, right: 0 }}>
        {PHONE_SECTIONS.map((section) => (
          <div key={section.name}>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 7,
                height: 30,
                padding: "0 18px",
                backgroundColor: section.tint,
              }}
            >
              <span style={{ width: 8, height: 8, borderRadius: 999, backgroundColor: section.dot }} />
              <span style={{ fontSize: 12, fontWeight: 600, color: C.text }}>{section.name}</span>
              <span style={{ fontSize: 11.5, color: C.dim }}>{section.count}</span>
            </div>
            {section.rows.map((row) => (
              <div
                key={row.id}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  height: 34,
                  padding: "0 18px",
                  borderBottom: `1px solid ${C.borderRow}`,
                }}
              >
                <span style={{ fontFamily: MONO_FONT, fontSize: 10, color: C.dim, flexShrink: 0 }}>{row.id}</span>
                <span style={{ fontSize: 12.5, color: C.text, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{row.title}</span>
              </div>
            ))}
          </div>
        ))}
      </div>
      {/* bottom tab bar */}
      <div
        style={{
          position: "absolute",
          left: 0,
          right: 0,
          bottom: 0,
          height: 62,
          borderTop: `1px solid ${C.border}`,
          backgroundColor: C.panel,
          display: "flex",
          alignItems: "center",
          justifyContent: "space-around",
          padding: "0 34px 10px",
        }}
      >
        <span style={{ color: C.text }}>
          <PhoneGlyph size={20}>
            <rect x="4" y="5" width="16" height="4" rx="1.2" />
            <rect x="4" y="13" width="16" height="4" rx="1.2" />
          </PhoneGlyph>
        </span>
        <span style={{ color: C.dim }}>
          <PhoneGlyph size={20}>
            <path d="M6 8a6 6 0 0 1 12 0c0 6 2.5 7.5 2.5 7.5h-17S6 14 6 8" />
            <path d="M10.3 20a2 2 0 0 0 3.4 0" />
          </PhoneGlyph>
        </span>
        <span style={{ color: C.dim }}>
          <PhoneGlyph size={20}>
            <circle cx="12" cy="12" r="3" />
            <path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M18.7 5.3l-2.1 2.1M7.4 16.6l-2.1 2.1" />
          </PhoneGlyph>
        </span>
      </div>
    </div>
  </div>
)

// ── The platform lineup (phase two) ──────────────────────────────────────────
const IconRow: React.FC<{ frame: number; at: number; icons: React.ReactNode[] }> = ({ frame, at, icons }) => (
  <div style={{ display: "flex", alignItems: "center", justifyContent: "center", gap: 40, color: "rgba(255,255,255,0.78)" }}>
    {icons.map((icon, i) => (
      <div key={i} style={{ ...rise(frame, at + i * 3, 9, 10) }}>
        {icon}
      </div>
    ))}
  </div>
)

const PlatformsScene: React.FC<{ frame: number; macScreen: React.ReactNode }> = ({ frame, macScreen }) => {
  const o = interpolate(frame, [ENDING.cardOutFrom + 6, ENDING.cardOutTo + 8], [0, 1], EASED)
  if (o <= 0) return null
  const phoneScale = 0.86
  const phoneH = (PHONE.screenH + 20) * phoneScale // ≈ 585
  return (
    <AbsoluteFill style={{ opacity: o }}>
      {/* left: the IDE inside a MacBook */}
      <div style={{ position: "absolute", left: 300, top: 238, ...rise(frame, ENDING.macAt, 12, 22) }}>
        <MacBook screenW={790}>{macScreen}</MacBook>
      </div>
      <div style={{ position: "absolute", left: 300, top: 848, width: 916, ...rise(frame, ENDING.iconsAt, 10) }}>
        <IconRow
          frame={frame}
          at={ENDING.iconsAt}
          icons={[<GlobeIcon key="web" size={34} />, <AppleIcon key="mac" size={34} />, <WindowsIcon key="win" size={32} />, <LinuxIcon key="linux" size={34} />]}
        />
      </div>

      {/* right: the mobile app inside an iPhone */}
      <div
        style={{
          position: "absolute",
          left: 1338,
          top: 238 + (594 - phoneH) / 2,
          transform: `scale(${phoneScale})`,
          transformOrigin: "top left",
          ...rise(frame, ENDING.phoneAt, 12, 22),
        }}
      >
        <PhoneMock />
      </div>
      <div style={{ position: "absolute", left: 1338, top: 848, width: PHONE.w * phoneScale, ...rise(frame, ENDING.iconsAt + 4, 10) }}>
        <IconRow frame={frame} at={ENDING.iconsAt + 4} icons={[<AppStoreIcon key="appstore" size={34} />, <PlayStoreIcon key="play" size={30} />]} />
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
