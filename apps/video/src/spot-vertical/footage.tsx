// spot-vertical/footage.tsx — the same picked Seedance takes, portrait-cropped.
// The takes are 960×960: covering 1080×1920 scales by HEIGHT, so the crop
// keeps only the middle 56% of the width — objectPosX is the tuning knob here
// (the 16:9 cut's objectPosY problem, rotated 90°).

import React from "react"
import { AbsoluteFill, OffthreadVideo, Sequence, interpolate, staticFile, useCurrentFrame } from "remotion"
import { EASE } from "../ships/theme"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

export type FootageSpecV = {
  file: string
  trimBeforeSec: number
  // horizontal crop anchor (object-position x%); a function gets the
  // sequence-LOCAL frame so anchor jumps hide inside the take's own cuts
  objectPosX?: number | ((localFrame: number) => number)
}

export const FOOTAGE_V = {
  chaos: {
    file: "footage/cafe-a.mp4",
    trimBeforeSec: 0.8, // same pick as the 16:9 cut: open mid-atmosphere
    // sit shot (he's left of center) until the take's own cut to the typing
    // macro at source ~2.0s = local f36
    objectPosX: (f: number) => interpolate(f, [32, 40], [38, 50], CLAMP),
  },
  payoff: {
    file: "footage/cafe-c.mp4",
    // 0.7: opens on the lean-back + lid half-close, cup already landed.
    // Shown window 0.7–3.2s — nowhere near the actor's line at ~6.5s.
    trimBeforeSec: 0.7,
    objectPosX: 42,
  },
} satisfies Record<string, FootageSpecV>

export const FootageClipV: React.FC<{
  spec: FootageSpecV
  from: number
  duration: number
  fadeIn?: number
  fadeOut?: number
  volume?: (localFrame: number) => number
}> = ({ spec, from, duration, fadeIn = 0, fadeOut = 0, volume }) => {
  const frame = useCurrentFrame() // composition-global
  if (frame < from || frame >= from + duration) return null
  const local = frame - from
  const o =
    interpolate(local, [0, Math.max(fadeIn, 0.01)], [fadeIn > 0 ? 0 : 1, 1], { ...CLAMP, easing: EASE }) *
    interpolate(local, [duration - Math.max(fadeOut, 0.01), duration], [1, fadeOut > 0 ? 0 : 1], { ...CLAMP, easing: EASE })
  const posX = typeof spec.objectPosX === "function" ? spec.objectPosX(local) : (spec.objectPosX ?? 50)
  return (
    <AbsoluteFill style={{ opacity: o }}>
      <Sequence from={from} durationInFrames={duration}>
        <OffthreadVideo
          src={staticFile(spec.file)}
          trimBefore={Math.round(spec.trimBeforeSec * 30)}
          volume={volume ?? 1}
          style={{
            width: "100%",
            height: "100%",
            objectFit: "cover",
            objectPosition: `${posX}% 50%`,
            filter: "saturate(0.92) contrast(1.04)",
          }}
        />
        <AbsoluteFill
          style={{
            background: "radial-gradient(ellipse at 50% 46%, transparent 55%, rgba(9,9,11,0.55) 100%)",
          }}
        />
      </Sequence>
    </AbsoluteFill>
  )
}
