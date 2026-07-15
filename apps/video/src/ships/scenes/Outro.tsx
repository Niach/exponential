// scenes/Outro.tsx — S12 brand card (storyboard §3 S12 / §2.9). Rendered in a
// Sequence from=1380, so useCurrentFrame() here is SEQUENCE-LOCAL (0–119);
// storyboard globals map as local = global − 1380 (f1386 → 6, f1400 → 20, …).

import React from "react"
import { AbsoluteFill, interpolate, useCurrentFrame } from "remotion"
import { C, EASE, MONO_FONT, UI_FONT } from "../theme"
import { COPY } from "../fixtures"
import { ExpLogo, useBlink } from "../rig"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const CLAMP_EASE = { ...CLAMP, easing: EASE } as const

const rise = (frame: number, from: number, to: number) => ({
  opacity: interpolate(frame, [from, to], [0, 1], CLAMP_EASE),
  translate: `0px ${interpolate(frame, [from, to], [16, 0], CLAMP_EASE)}px`,
})

export const Outro: React.FC<{ fadeOutAt?: number; tagline?: string; urlIn?: number }> = ({
  fadeOutAt = 112,
  tagline: taglineText = COPY.tagline,
  urlIn = 44,
}) => {
  const frame = useCurrentFrame() // sequence-local
  const blinkOn = useBlink(frame)

  const discO = interpolate(frame, [6, 14], [0, 1], CLAMP_EASE)
  const drawT = interpolate(frame, [12, 34], [0, 1], CLAMP_EASE)
  const wordmark = rise(frame, 20, 36)
  const tagline = rise(frame, 28, 44)
  const urlO = interpolate(frame, [urlIn, urlIn + 12], [0, 1], CLAMP_EASE)
  const fade = interpolate(frame, [fadeOutAt, fadeOutAt + 8], [1, 0], CLAMP)

  return (
    <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", opacity: fade }}>
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 28 }}>
        <ExpLogo size={118} drawT={drawT} discO={discO} />
        <div
          style={{
            fontFamily: UI_FONT,
            fontSize: 96,
            fontWeight: 700,
            letterSpacing: -3,
            color: C.text,
            lineHeight: 1.05,
            ...wordmark,
          }}
        >
          Exponential
        </div>
        <div
          style={{
            fontFamily: UI_FONT,
            fontSize: 36,
            fontWeight: 500,
            color: C.muted,
            lineHeight: 1.2,
            ...tagline,
          }}
        >
          {taglineText}
        </div>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            fontFamily: MONO_FONT,
            fontSize: 28,
            color: C.muted,
            opacity: urlO,
          }}
        >
          {COPY.url}
          <span
            style={{
              display: "inline-block",
              width: 15,
              height: 28,
              marginLeft: 10,
              backgroundColor: C.text,
              opacity: blinkOn ? 1 : 0,
            }}
          />
        </div>
      </div>
    </AbsoluteFill>
  )
}
