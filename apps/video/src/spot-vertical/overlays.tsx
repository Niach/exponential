// spot-vertical/overlays.tsx — portrait full-frame type. Deliberate near-copy
// of spot/overlays.tsx (the 16:9 cut stays frozen; this folder owns its look):
// same opaque-plateau contract, portrait type sizes and stacking.

import React from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { ExpLogo } from "../ships/rig"
import { C, EASE, UI_FONT } from "../ships/theme"

const CLAMP_EASE = { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: EASE } as const

export const TypeCardV: React.FC<{
  frame: number
  in: number
  out: number
  brand?: boolean
  children: React.ReactNode
}> = ({ frame, in: fin, out, brand = false, children }) => {
  if (frame < fin || frame > out) return null
  const bgO = interpolate(frame, [fin, fin + 8, out - 8, out], [0, 1, 1, 0], CLAMP_EASE)
  const textO = interpolate(frame, [fin + 6, fin + 14, out - 14, out - 6], [0, 1, 1, 0], CLAMP_EASE)
  const sc = interpolate(frame, [fin + 6, fin + 14], [1.03, 1], CLAMP_EASE)
  return (
    <AbsoluteFill style={{ opacity: bgO }}>
      <AbsoluteFill style={{ backgroundColor: C.canvas }} />
      <AbsoluteFill
        style={{
          background: `radial-gradient(ellipse 80% 30% at 50% 45%, rgba(99,102,241,0.16), transparent 70%)`,
        }}
      />
      <AbsoluteFill style={{ alignItems: "center", justifyContent: "center" }}>
        {brand ? (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 14,
              marginBottom: 44,
              opacity: textO,
            }}
          >
            <ExpLogo size={42} />
            <span
              style={{
                fontFamily: UI_FONT,
                fontSize: 36,
                fontWeight: 600,
                letterSpacing: -0.5,
                color: C.muted,
              }}
            >
              Exponential
            </span>
          </div>
        ) : null}
        <div
          style={{
            fontFamily: UI_FONT,
            fontSize: 92,
            fontWeight: 700,
            letterSpacing: -2,
            color: C.text,
            textAlign: "center",
            lineHeight: 1.12,
            opacity: textO,
            scale: String(sc),
            textShadow: "0 2px 40px rgba(0,0,0,0.6)",
            padding: "0 40px",
          }}
        >
          {children}
        </div>
      </AbsoluteFill>
    </AbsoluteFill>
  )
}

// Portrait punch — lower-third text over footage (Punch in ships/rig is
// calibrated to the 1080-high canvas; this one owns the 1920-high geometry).
export const PunchV: React.FC<{
  frame: number
  in: number
  out: number
  lines: string[]
  size?: number
  y?: number
}> = ({ frame, in: fin, out, lines, size = 64, y = 1520 }) => {
  if (frame < fin - 2 || frame > out + 8) return null
  const o = interpolate(frame, [fin, fin + 8, out, out + 6], [0, 1, 1, 0], CLAMP_EASE)
  const sc = interpolate(frame, [fin, fin + 8], [1.04, 1], CLAMP_EASE)
  return (
    <>
      <AbsoluteFill
        style={{
          background: `linear-gradient(to bottom, transparent 55%, rgba(9,9,11,0.72) 88%)`,
          opacity: o,
        }}
      />
      <div
        style={{
          position: "absolute",
          left: 0,
          right: 0,
          top: y,
          textAlign: "center",
          fontFamily: UI_FONT,
          fontSize: size,
          fontWeight: 700,
          letterSpacing: -1.5,
          color: C.text,
          opacity: o,
          scale: String(sc),
          lineHeight: 1.14,
          textShadow: "0 2px 32px rgba(0,0,0,0.8)",
          padding: "0 48px",
        }}
      >
        {lines.map((l) => (
          <div key={l}>{l}</div>
        ))}
      </div>
    </>
  )
}
