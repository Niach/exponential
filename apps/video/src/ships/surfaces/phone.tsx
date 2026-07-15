// PhonePiP — iPhone chassis + the real iOS steer activity screen (screen-space PiP).
// Pixel truth: apps/video/ref/ios-steer-activity.png. Content truth: fixtures.PHONE_FEED.
// The assembler positions it via x/y/rotate and drives reveals via feedSchedule
// (composition-global frames, one per PHONE_FEED item, each 2f after its desktop line).

import React from "react"
import { interpolate } from "remotion"
import { C, EASE, MONO_FONT, UI_FONT } from "../theme"
import { IDENTITY, PHONE_DIFFSTAT, PHONE_FEED, SteerItem } from "../fixtures"

// ── Phone-local palette (sampled from ref/ios-steer-activity.png — iOS material
//    grays that intentionally do not exist in the shared desktop theme) ─────────
const P = {
  chassis: "#050505",
  chassisBorder: "rgba(255,255,255,0.14)",
  chrome: "#262628", // status bar + Live header band
  feedBg: "#121214", // activity feed area
  dock: "#2b2b2d", // bottom dock (strip + composer + home)
  stripBg: "#303032", // "Latest changes" row
  bubbleBg: "#242427", // narration card
  bubbleBorder: "rgba(255,255,255,0.09)",
  inputBg: "#37373a",
  inputBorder: "rgba(255,255,255,0.07)",
  pillBg: "rgba(84,84,88,0.94)", // "Jump to latest" floating pill
  xBtnBg: "#3a3a3d",
  sendBg: "rgba(255,255,255,0.12)",
  textHi: "rgba(255,255,255,0.94)",
  textMid: "rgba(255,255,255,0.72)",
  textLow: "rgba(255,255,255,0.45)",
  placeholder: "rgba(255,255,255,0.38)",
} as const

// ── Geometry (screen-local px; screen 312×660 inside the 330-wide chassis) ─────
const SCREEN_W = 312
const SCREEN_H = 660
const CHROME_H = 80 // status row (0–42) + Live header (42–80)
const DOCK_TOP = 560
const TOOL_ROW_H = 21
const TOOL_GAP = 3
const LINE_H = 19
const BUBBLE_LEFT = 26 // bubble x (sparkle gutter to its left)
const BUBBLE_RIGHT = 10
const BUBBLE_PAD_X = 11
const BUBBLE_PAD_Y = 8
const BUBBLE_MARGIN_V = 6
const NARR_MAX_PX = 250 // wrap width inside the bubble (slightly conservative)

export const PHONE_SIZE = { w: 330, h: SCREEN_H + 20 } as const

// ── Deterministic text wrap (fixed strings + fixed width ⇒ fixed line count).
//    Lines render as individual nowrap divs, so item heights are exact. ────────
const charEm = (ch: string): number => {
  if (ch === " ") return 0.28
  if ("iljt.,':;!|`".includes(ch)) return 0.3
  if ("fr-()[]".includes(ch)) return 0.38
  if ("mwMW@—#".includes(ch)) return 0.92
  if (ch >= "A" && ch <= "Z") return 0.68
  if (ch >= "0" && ch <= "9") return 0.62
  return 0.55
}
const textPx = (s: string, fontPx: number): number => {
  let w = 0
  for (const ch of s) w += charEm(ch)
  return w * fontPx
}
const wrapLines = (text: string, maxPx: number, fontPx: number): string[] => {
  const lines: string[] = []
  let cur = ""
  for (const word of text.split(" ")) {
    const cand = cur === "" ? word : `${cur} ${word}`
    if (cur === "" || textPx(cand, fontPx) <= maxPx) cur = cand
    else {
      lines.push(cur)
      cur = word
    }
  }
  if (cur !== "") lines.push(cur)
  return lines
}

type FeedEntry = { item: SteerItem; h: number; lines: string[] }
const FEED_LAYOUT: FeedEntry[] = PHONE_FEED.map((item) => {
  if (item.kind === "tool") return { item, h: TOOL_ROW_H + TOOL_GAP, lines: [] }
  const lines = wrapLines(item.text, NARR_MAX_PX, 13)
  return { item, h: lines.length * LINE_H + BUBBLE_PAD_Y * 2 + BUBBLE_MARGIN_V * 2, lines }
})

// ── Tiny inline glyphs (lucide-ish, stroke 1.6–2, currentColor) ────────────────
const CrossedToolsGlyph: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d="M7.4 7.4 19 19" />
    <path d="M8.8 3.6a4.2 4.2 0 1 0-5.2 5.2" />
    <path d="M16.6 7.4 5 19" />
    <path d="M15.6 4.2l4.2 4.2" />
  </svg>
)

const SparkleGlyph: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor">
    <path d="M12 4l1.7 6.3L20 12l-6.3 1.7L12 20l-1.7-6.3L4 12l6.3-1.7Z" />
    <path d="M18.5 2.5l.7 2.3 2.3.7-2.3.7-.7 2.3-.7-2.3-2.3-.7 2.3-.7Z" opacity={0.8} />
  </svg>
)

const MoonGlyph: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor">
    <path d="M20.6 14.4A8.7 8.7 0 1 1 9.6 3.4a7 7 0 0 0 11 11Z" />
  </svg>
)

const WifiGlyph: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round">
    <path d="M3 9.5C8.2 4.6 15.8 4.6 21 9.5" />
    <path d="M6.2 13.2c3.4-3.1 8.2-3.1 11.6 0" />
    <path d="M9.4 16.8c1.5-1.4 3.7-1.4 5.2 0" />
    <circle cx={12} cy={20} r={1.4} fill="currentColor" stroke="none" />
  </svg>
)

const BatteryGlyph: React.FC = () => (
  <span style={{ display: "flex", alignItems: "center" }}>
    <span
      style={{
        width: 25,
        height: 13,
        borderRadius: 4,
        backgroundColor: "rgba(255,255,255,0.88)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        fontFamily: UI_FONT,
        fontSize: 9.5,
        fontWeight: 700,
        color: "#1c1c1e",
        letterSpacing: -0.2,
      }}
    >
      73
    </span>
    <span style={{ width: 1.5, height: 4.5, marginLeft: 1, borderRadius: "0 2px 2px 0", backgroundColor: "rgba(255,255,255,0.5)" }} />
  </span>
)

const XGlyph: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round">
    <path d="M6 6l12 12M18 6L6 18" />
  </svg>
)

const DiffGlyph: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round">
    <path d="M6.5 3.5v6M3.5 6.5h6" />
    <path d="M15.5 3.5l-7 17" />
    <path d="M14.5 17.5h6" />
  </svg>
)

const ChevronUpGlyph: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round">
    <path d="M6 14.5l6-6 6 6" />
  </svg>
)

const ArrowUpGlyph: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.6} strokeLinecap="round" strokeLinejoin="round">
    <path d="M12 19V5" />
    <path d="M5.5 11.5 12 5l6.5 6.5" />
  </svg>
)

const ArrowDownGlyph: React.FC<{ size: number }> = ({ size }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round">
    <path d="M12 5v14" />
    <path d="M5.5 12.5 12 19l6.5-6.5" />
  </svg>
)

// ── Feed items ─────────────────────────────────────────────────────────────────
const ToolRow: React.FC<{ name: string; summary?: string }> = ({ name, summary }) => (
  <div style={{ display: "flex", alignItems: "center", height: TOOL_ROW_H, paddingLeft: 10, paddingRight: 10 }}>
    <span style={{ width: 16, flex: "none", display: "flex", justifyContent: "center", color: "rgba(255,255,255,0.55)" }}>
      <CrossedToolsGlyph size={12} />
    </span>
    <span
      style={{
        marginLeft: 7,
        fontFamily: UI_FONT,
        fontSize: 13,
        fontWeight: 700,
        color: P.textHi,
        whiteSpace: "nowrap",
        overflow: "hidden",
        textOverflow: "ellipsis",
        flex: "0 1 auto",
        minWidth: 0,
      }}
    >
      {name}
    </span>
    {summary === undefined ? null : (
      <span
        style={{
          marginLeft: 8,
          fontFamily: MONO_FONT,
          fontSize: 11,
          color: P.textLow,
          whiteSpace: "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
          flex: "1 1 0%",
          minWidth: 0,
        }}
      >
        {summary}
      </span>
    )}
  </div>
)

const NarrationBubble: React.FC<{ lines: string[] }> = ({ lines }) => (
  <div style={{ display: "flex", alignItems: "flex-start", paddingTop: BUBBLE_MARGIN_V, paddingBottom: BUBBLE_MARGIN_V }}>
    <span style={{ width: BUBBLE_LEFT, flex: "none", display: "flex", justifyContent: "center", paddingTop: 3, color: "rgba(255,255,255,0.6)" }}>
      <SparkleGlyph size={12} />
    </span>
    <div
      style={{
        flex: "1 1 0%",
        minWidth: 0,
        marginRight: BUBBLE_RIGHT,
        borderRadius: 16,
        backgroundColor: P.bubbleBg,
        border: `1px solid ${P.bubbleBorder}`,
        padding: `${BUBBLE_PAD_Y - 1}px ${BUBBLE_PAD_X - 1}px`, // -1: border included in fixed height
      }}
    >
      {lines.map((l) => (
        <div key={l} style={{ fontFamily: UI_FONT, fontSize: 13, lineHeight: `${LINE_H}px`, color: P.textHi, whiteSpace: "pre" }}>
          {l}
        </div>
      ))}
    </div>
  </div>
)

// ── The surface ────────────────────────────────────────────────────────────────
export type PhonePiPProps = {
  frame: number // composition-global frame
  x: number // comp-space left of the chassis
  y: number // comp-space top of the chassis
  rotate: number // degrees
  feedSchedule: number[] // global reveal frame per PHONE_FEED item (missing ⇒ hidden)
  sendPulseAt?: number // soft green glow pulse on the circular send button
  jumpPillAt?: number // optional: global frame the "Jump to latest" pill fades in (default: auto, once the feed has built up)
  opacity?: number // whole-PiP fade (assembler exit)
}

export const PhonePiP: React.FC<PhonePiPProps> = ({ frame, x, y, rotate, feedSchedule, sendPulseAt, jumpPillAt, opacity = 1 }) => {
  const clampEase = { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: EASE } as const

  // Revealed content height (drives the "Jump to latest" pill fade-in).
  const viewH = DOCK_TOP - CHROME_H
  let contentH = 10
  for (let i = 0; i < FEED_LAYOUT.length; i++) {
    const t = feedSchedule[i]
    if (t === undefined) continue
    contentH += interpolate(frame, [t, t + 7], [0, FEED_LAYOUT[i].h], clampEase)
  }
  const pillO =
    jumpPillAt === undefined
      ? interpolate(contentH, [190, 270], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })
      : interpolate(frame, [jumpPillAt, jumpPillAt + 8], [0, 1], clampEase)

  const pulse = sendPulseAt === undefined ? 0 : interpolate(frame, [sendPulseAt, sendPulseAt + 7, sendPulseAt + 26], [0, 1, 0], clampEase)

  return (
    <div style={{ position: "absolute", left: x, top: y, width: PHONE_SIZE.w, rotate: `${rotate}deg`, transformOrigin: "50% 50%", opacity }}>
      {/* chassis */}
      <div
        style={{
          padding: 9,
          borderRadius: 44,
          backgroundColor: P.chassis,
          border: `1px solid ${P.chassisBorder}`,
          boxShadow: "0 30px 70px rgba(0,0,0,0.55), 0 4px 18px rgba(0,0,0,0.4)",
        }}
      >
        {/* screen */}
        <div style={{ position: "relative", width: SCREEN_W, height: SCREEN_H, borderRadius: 36, overflow: "hidden", backgroundColor: P.feedBg, fontFamily: UI_FONT }}>
          {/* feed viewport (bottom-anchored: newest stays visible, older clips under the chrome) */}
          <div style={{ position: "absolute", top: CHROME_H, left: 0, right: 0, height: viewH, overflow: "hidden" }}>
            <div style={{ position: "absolute", left: 0, right: 0, bottom: 10 }}>
              {FEED_LAYOUT.map((entry, i) => {
                const t = feedSchedule[i]
                if (t === undefined || frame < t) return null
                const h = interpolate(frame, [t, t + 7], [0, entry.h], clampEase)
                const o = interpolate(frame, [t + 1, t + 6], [0, 1], clampEase)
                const rise = interpolate(frame, [t, t + 7], [6, 0], clampEase)
                return (
                  <div key={`${entry.item.kind}-${i}`} style={{ height: h, display: "flex", flexDirection: "column", justifyContent: "flex-end", overflow: "visible" }}>
                    <div style={{ opacity: o, translate: `0px ${rise}px` }}>
                      {entry.item.kind === "tool" ? (
                        <div style={{ paddingBottom: TOOL_GAP }}>
                          <ToolRow name={entry.item.name} summary={entry.item.summary} />
                        </div>
                      ) : (
                        <NarrationBubble lines={entry.lines} />
                      )}
                    </div>
                  </div>
                )
              })}
            </div>
          </div>

          {/* top chrome: status bar + Live header */}
          <div style={{ position: "absolute", top: 0, left: 0, right: 0, height: CHROME_H, backgroundColor: P.chrome, zIndex: 2 }}>
            {/* status row */}
            <div style={{ position: "absolute", top: 6, left: 26, right: 26, height: 34, display: "flex", alignItems: "center", justifyContent: "space-between" }}>
              <span style={{ display: "flex", alignItems: "center", gap: 3, fontSize: 14, fontWeight: 600, color: C.text, letterSpacing: 0.2 }}>
                23:39
                <span style={{ display: "flex", color: C.text }}>
                  <MoonGlyph size={13} />
                </span>
              </span>
              <span style={{ display: "flex", alignItems: "center", gap: 5, color: C.text }}>
                <WifiGlyph size={15} />
                <BatteryGlyph />
              </span>
            </div>
            {/* Live header */}
            <div style={{ position: "absolute", top: 42, left: 12, right: 12, height: 38, display: "flex", alignItems: "center", gap: 8 }}>
              <span style={{ width: 9, height: 9, flex: "none", borderRadius: 999, backgroundColor: C.green }} />
              <span style={{ flex: "1 1 0%", minWidth: 0, fontSize: 13, color: P.textMid, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                Live · {IDENTITY.host}
              </span>
              <span
                style={{
                  width: 26,
                  height: 26,
                  flex: "none",
                  borderRadius: 999,
                  backgroundColor: P.xBtnBg,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  color: "rgba(255,255,255,0.82)",
                }}
              >
                <XGlyph size={11} />
              </span>
            </div>
          </div>

          {/* dynamic island */}
          <div
            style={{
              position: "absolute",
              top: 9,
              left: (SCREEN_W - 88) / 2,
              width: 88,
              height: 25,
              borderRadius: 999,
              backgroundColor: "#000000",
              border: "1px solid rgba(255,255,255,0.05)",
              zIndex: 4,
            }}
          />

          {/* floating "Jump to latest" pill */}
          <div style={{ position: "absolute", left: 0, right: 0, top: DOCK_TOP - 34, display: "flex", justifyContent: "center", zIndex: 3, opacity: pillO }}>
            <span
              style={{
                display: "flex",
                alignItems: "center",
                gap: 5,
                padding: "5px 12px",
                borderRadius: 999,
                backgroundColor: P.pillBg,
                boxShadow: "0 4px 14px rgba(0,0,0,0.35)",
                fontSize: 11.5,
                fontWeight: 600,
                color: C.text,
              }}
            >
              Jump to latest
              <ArrowDownGlyph size={10} />
            </span>
          </div>

          {/* bottom dock: Latest changes strip + composer + home indicator */}
          <div style={{ position: "absolute", top: DOCK_TOP, left: 0, right: 0, bottom: 0, backgroundColor: P.dock, zIndex: 2 }}>
            {/* pinned "Latest changes" strip */}
            <div
              style={{
                position: "absolute",
                top: 8,
                left: 10,
                right: 10,
                height: 26,
                borderRadius: 10,
                backgroundColor: P.stripBg,
                display: "flex",
                alignItems: "center",
                padding: "0 10px",
                gap: 7,
              }}
            >
              <span style={{ display: "flex", color: "rgba(255,255,255,0.85)" }}>
                <DiffGlyph size={13} />
              </span>
              <span style={{ flex: "1 1 0%", fontSize: 12, fontWeight: 600, color: C.text }}>Latest changes</span>
              <span style={{ fontSize: 12, fontWeight: 600, color: C.diffAdd }}>+{PHONE_DIFFSTAT.add}</span>
              <span style={{ fontSize: 12, fontWeight: 600, color: C.diffDel }}>−{PHONE_DIFFSTAT.del}</span>
              <span style={{ display: "flex", color: "rgba(255,255,255,0.8)", marginLeft: 2 }}>
                <ChevronUpGlyph size={12} />
              </span>
            </div>
            {/* composer */}
            <div
              style={{
                position: "absolute",
                top: 42,
                left: 10,
                right: 52,
                height: 28,
                borderRadius: 15,
                backgroundColor: P.inputBg,
                border: `1px solid ${P.inputBorder}`,
                display: "flex",
                alignItems: "center",
                padding: "0 12px",
                fontSize: 13,
                color: P.placeholder,
                whiteSpace: "nowrap",
              }}
            >
              Message the agent…
            </div>
            <div
              style={{
                position: "absolute",
                top: 42,
                right: 10,
                width: 28,
                height: 28,
                borderRadius: 999,
                backgroundColor: P.sendBg,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                color: `rgba(255,255,255,${0.6 + 0.35 * pulse})`,
                scale: String(1 + 0.06 * pulse),
                boxShadow:
                  pulse > 0
                    ? `0 0 0 ${1 + 4 * pulse}px rgba(34,197,94,${0.4 * pulse}), 0 0 ${16 * pulse}px 2px rgba(34,197,94,${0.35 * pulse})`
                    : undefined,
              }}
            >
              <ArrowUpGlyph size={14} />
            </div>
            {/* home indicator */}
            <div
              style={{
                position: "absolute",
                bottom: 7,
                left: (SCREEN_W - 118) / 2,
                width: 118,
                height: 4,
                borderRadius: 999,
                backgroundColor: "rgba(255,255,255,0.35)",
              }}
            />
          </div>
        </div>
      </div>
    </div>
  )
}
