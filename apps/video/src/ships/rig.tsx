// Global rig: camera, cursor, overlay system, brand mark, animation helpers.
// Storyboard §0. All frame values passed to these helpers are COMPOSITION-GLOBAL
// frames unless a component is documented as sequence-local.

import React, { useId } from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { C, EASE, UI_FONT, WIN } from "./theme"

// ── Camera ────────────────────────────────────────────────────────────────────
// Keyframes reference window-local focus points; the camera keeps focus at comp
// center: translate = (960,540) − s·(focus + windowOrigin). transformOrigin 0 0.
export type CamKey = { f: number; s: number; x: number; y: number; ease?: "ease" | "linear" }

export const camAt = (keys: CamKey[], frame: number) => {
  if (keys.length === 0) return { s: 1, x: WIN.w / 2, y: WIN.h / 2 }
  let prev = keys[0]
  let next = keys[keys.length - 1]
  for (let i = 0; i < keys.length; i++) {
    if (keys[i].f <= frame) prev = keys[i]
    if (keys[i].f >= frame) {
      next = keys[i]
      break
    }
  }
  if (prev === next || next.f === prev.f) return { s: prev.s, x: prev.x, y: prev.y }
  const opts = {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    ...(next.ease === "linear" ? {} : { easing: EASE }),
  } as const
  return {
    s: interpolate(frame, [prev.f, next.f], [prev.s, next.s], opts),
    x: interpolate(frame, [prev.f, next.f], [prev.x, next.x], opts),
    y: interpolate(frame, [prev.f, next.f], [prev.y, next.y], opts),
  }
}

// Wrap the window layer (which renders at comp coords WIN.x/WIN.y). Children in
// window-local coords should be inside <DesktopWindow>. `frame` must be global.
export const Camera: React.FC<{ keys: CamKey[]; frame: number; children: React.ReactNode }> = ({
  keys,
  frame,
  children,
}) => {
  const { s, x, y } = camAt(keys, frame)
  return (
    <AbsoluteFill
      style={{
        transformOrigin: "0 0",
        translate: `${960 - s * (x + WIN.x)}px ${540 - s * (y + WIN.y)}px`,
        scale: String(s),
      }}
    >
      {children}
    </AbsoluteFill>
  )
}

// The desktop window chassis at comp coords — put window-local content inside.
export const WindowChassis: React.FC<{ children: React.ReactNode; dim?: number }> = ({
  children,
  dim = 0,
}) => (
  <div
    style={{
      position: "absolute",
      left: WIN.x,
      top: WIN.y,
      width: WIN.w,
      height: WIN.h,
      borderRadius: WIN.radius,
      border: `1px solid ${C.border}`,
      boxShadow: "0 40px 120px rgba(0,0,0,0.6), 0 8px 24px rgba(0,0,0,0.4)",
      backgroundColor: C.bg,
      overflow: "hidden",
      filter: dim > 0 ? `brightness(${1 - dim})` : undefined,
    }}
  >
    {children}
  </div>
)

// ── Cursor (window-local layer; scales with the camera) ─────────────────────
export type CursorKey = { f: number; x: number; y: number }
export const CursorLayer: React.FC<{
  keys: CursorKey[]
  clicks?: number[] // global frames
  frame: number
  from?: number
  to?: number
}> = ({ keys, clicks = [], frame, from = 0, to = Infinity }) => {
  if (frame < from || frame > to || keys.length === 0) return null
  let prev = keys[0]
  let next = keys[keys.length - 1]
  for (const k of keys) {
    if (k.f <= frame) prev = k
    if (k.f >= frame) {
      next = k
      break
    }
  }
  const t =
    prev === next || next.f === prev.f
      ? { x: prev.x, y: prev.y }
      : {
          x: interpolate(frame, [prev.f, next.f], [prev.x, next.x], { easing: EASE, extrapolateLeft: "clamp", extrapolateRight: "clamp" }),
          y: interpolate(frame, [prev.f, next.f], [prev.y, next.y], { easing: EASE, extrapolateLeft: "clamp", extrapolateRight: "clamp" }),
        }
  const click = clicks.find((c) => frame >= c && frame < c + 8)
  const clickT = click === undefined ? 0 : (frame - click) / 8
  const pressScale = click === undefined ? 1 : interpolate(frame, [click, click + 2, click + 4], [1, 0.88, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })
  return (
    <div style={{ position: "absolute", left: t.x, top: t.y, zIndex: 90 }}>
      {click !== undefined ? (
        <div
          style={{
            position: "absolute",
            left: -6 - clickT * 12,
            top: -6 - clickT * 12,
            width: 12 + clickT * 24,
            height: 12 + clickT * 24,
            borderRadius: "50%",
            border: `2px solid rgba(255,255,255,${0.4 * (1 - clickT)})`,
          }}
        />
      ) : null}
      <svg width={22} height={22} viewBox="0 0 24 24" style={{ scale: String(pressScale), filter: "drop-shadow(0 2px 4px rgba(0,0,0,0.6))" }}>
        <path d="M5 3 L19 12.5 L12.6 13.8 L15.5 20 L13 21 L10.2 14.8 L5 19 Z" fill="#fafafa" stroke="#111" strokeWidth={1.2} strokeLinejoin="round" />
      </svg>
    </div>
  )
}

// ── Overlay system (screen-space, sequence-local frames NOT used — pass global) ─
const Scrim: React.FC<{ o: number }> = ({ o }) => (
  <div
    style={{
      position: "absolute",
      left: 0,
      right: 0,
      bottom: 0,
      height: 220,
      opacity: o,
      background: "linear-gradient(to top, rgba(0,0,0,0.55), transparent)",
    }}
  />
)

// in/out are GLOBAL frames; render inside the screen-space layer.
export const Caption: React.FC<{
  frame: number
  in: number
  out: number
  children: React.ReactNode
  size?: number
  centered?: boolean
}> = ({ frame, in: fin, out, children, size = 44, centered = false }) => {
  if (frame < fin - 2 || frame > out + 8) return null
  const o = interpolate(frame, [fin, fin + 8, out, out + 6], [0, 1, 1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: EASE })
  const sc = interpolate(frame, [fin, fin + 8], [1.04, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: EASE })
  return (
    <>
      <Scrim o={o} />
      <div
        style={{
          position: "absolute",
          left: centered ? 0 : 120,
          right: centered ? 0 : undefined,
          bottom: 1080 - 1020,
          textAlign: centered ? "center" : "left",
          fontFamily: UI_FONT,
          fontSize: size,
          fontWeight: 600,
          letterSpacing: -0.5,
          color: C.text,
          opacity: o,
          scale: String(sc),
          lineHeight: 1.25,
          textShadow: "0 2px 24px rgba(0,0,0,0.7)",
        }}
      >
        {children}
      </div>
    </>
  )
}

export const Punch: React.FC<{ frame: number; in: number; out: number; lines: string[]; size?: number; weight?: number; y?: number }> = ({
  frame,
  in: fin,
  out,
  lines,
  size = 64,
  weight = 650,
  y = 880,
}) => {
  if (frame < fin - 2 || frame > out + 8) return null
  const o = interpolate(frame, [fin, fin + 8, out, out + 6], [0, 1, 1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: EASE })
  const sc = interpolate(frame, [fin, fin + 8], [1.04, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: EASE })
  return (
    <>
      <Scrim o={o} />
      <div
        style={{
          position: "absolute",
          left: 0,
          right: 0,
          top: y,
          textAlign: "center",
          fontFamily: UI_FONT,
          fontSize: size,
          fontWeight: weight,
          letterSpacing: -1.5,
          color: C.text,
          opacity: o,
          scale: String(sc),
          lineHeight: 1.12,
          textShadow: "0 2px 32px rgba(0,0,0,0.8)",
        }}
      >
        {lines.map((l) => (
          <div key={l}>{l}</div>
        ))}
      </div>
    </>
  )
}

// ── Brand ─────────────────────────────────────────────────────────────────────
// Real cut-curve logo (video-brand.md §3). drawT: 0→1 stroke-reveals the curves.
export const ExpLogo: React.FC<{ size: number; drawT?: number; discO?: number }> = ({ size, drawT = 1, discO = 1 }) => {
  const uid = useId().replace(/[^a-zA-Z0-9]/g, "")
  const curves = [
    "M -5.87 62.01 C 39.09 65.44 48.72 28.71 49.03 -6.21",
    "M -5.07 86.00 C 53.78 84.42 71.13 37.29 73.00 -5.09",
    "M -4.27 109.99 C 68.46 103.40 93.55 45.86 96.98 -3.98",
  ]
  const LEN = 140 // safe overestimate of each curve's path length
  return (
    <svg width={size} height={size} viewBox="0 0 100 100">
      <defs>
        <clipPath id={`c${uid}`}>
          <circle cx="50" cy="50" r="50" />
        </clipPath>
        <mask id={`m${uid}`}>
          <rect width="100" height="100" fill="white" />
          <g clipPath={`url(#c${uid})`}>
            {curves.map((d, i) => {
              const t = Math.min(1, Math.max(0, drawT * 3 - i * 0.35))
              return (
                <path key={d} d={d} stroke="black" strokeWidth={6} fill="none" strokeDasharray={LEN} strokeDashoffset={LEN * (1 - t)} />
              )
            })}
          </g>
        </mask>
      </defs>
      <circle cx="50" cy="50" r="50" fill="#ffffff" opacity={discO} mask={`url(#m${uid})`} />
    </svg>
  )
}

export const WordmarkChip: React.FC<{ frame: number; in: number; out: number }> = ({ frame, in: fin, out }) => {
  if (frame < fin || frame > out + 10) return null
  const o = interpolate(frame, [fin, fin + 10, out, out + 10], [0, 0.8, 0.8, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })
  return (
    <div
      style={{
        position: "absolute",
        left: 120,
        top: 1024, // below the caption zone (captions bottom out at y 1020)
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "6px 14px 6px 10px",
        borderRadius: 999,
        backgroundColor: C.panel,
        border: `1px solid ${C.border}`,
        opacity: o,
      }}
    >
      <ExpLogo size={20} />
      <span style={{ fontFamily: UI_FONT, fontSize: 15, fontWeight: 600, color: C.text }}>Exponential</span>
    </div>
  )
}

// ── Small helpers ─────────────────────────────────────────────────────────────
export const useBlink = (frame: number) => frame % 16 < 8

// Typed prefix of `text`, starting at global frame `start`, cps chars/frame.
export const typed = (text: string, frame: number, start: number, cpf = 2) =>
  frame < start ? "" : text.slice(0, Math.max(0, Math.floor((frame - start) * cpf)))

// Fade+rise entrance: returns {opacity, translate} style fragment.
export const riseIn = (frame: number, start: number, dur = 9, rise = 12) => ({
  opacity: interpolate(frame, [start, start + dur], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: EASE }),
  translate: `0px ${interpolate(frame, [start, start + dur], [rise, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: EASE })}px`,
})

// Rolling integer (digit roll): counts from `from` to `to` over [start, end].
export const rollNum = (frame: number, start: number, end: number, from: number, to: number) =>
  Math.round(interpolate(frame, [start, end], [from, to], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }))
