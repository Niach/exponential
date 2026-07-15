// spot/overlays.tsx — the full-frame type cards (storyboard-launch-spot.md §4).
// A card is an OPAQUE canvas cover: its plateau is where camera cuts and shell
// switches hide. Text enters 6f after the bg, exits 6f before it lifts.

import React from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { ExpLogo } from "../ships/rig"
import { C, EASE, UI_FONT } from "../ships/theme"

const CLAMP_EASE = { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: EASE } as const

export const TypeCard: React.FC<{
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
      {/* its own soft glow — the global Background is hidden while covered */}
      <AbsoluteFill
        style={{
          background: `radial-gradient(ellipse 60% 45% at 50% 42%, rgba(99,102,241,0.16), transparent 70%)`,
        }}
      />
      <AbsoluteFill style={{ alignItems: "center", justifyContent: "center" }}>
        {brand ? (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 16,
              marginBottom: 40,
              opacity: textO,
            }}
          >
            <ExpLogo size={44} />
            <span
              style={{
                fontFamily: UI_FONT,
                fontSize: 38,
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
            fontSize: 100,
            fontWeight: 700,
            letterSpacing: -2,
            color: C.text,
            textAlign: "center",
            lineHeight: 1.1,
            opacity: textO,
            scale: String(sc),
            textShadow: "0 2px 40px rgba(0,0,0,0.6)",
          }}
        >
          {children}
        </div>
      </AbsoluteFill>
    </AbsoluteFill>
  )
}
