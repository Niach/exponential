// acts/Ignition.tsx — cold open (0–2.6 s): a single live dot lands on black,
// rings out, and hands its centre to the brand mark; the lockup itself is the
// root's `Lockup` (it survives every act as the corner chip). This act owns
// the dot, the ripples, the hairline and the tagline.

import React from "react"
import { AbsoluteFill } from "remotion"
import { C, H, UI, W } from "../theme"
import { IN, SNAP, mix, pop, seg } from "../motion"
import { Ripple } from "../atoms"

const CX = W / 2
const CY = H / 2

export const Ignition: React.FC<{ f: number }> = ({ f }) => {
  const dotS = pop(f, 0, SNAP)
  const dotO = 1 - seg(f, 12, 18)
  const dotSize = 22

  // Hairline under the lockup: draws out from the centre, then lets go.
  const lineW = mix(f, 34, 52, 0, 900)
  const lineO = (1 - seg(f, 56, 64, IN)) * 0.9

  const tagIn = seg(f, 38, 54)
  const tagOut = seg(f, 56, 64, IN)

  return (
    <AbsoluteFill>
      {[2, 8, 14].map((at, i) => (
        <Ripple
          key={at}
          f={f}
          at={at}
          x={CX}
          y={CY}
          size={220 + i * 90}
          color={i === 0 ? C.green : `rgba(255,255,255,0.5)`}
          dur={26}
          width={i === 0 ? 2 : 1}
        />
      ))}
      {dotO > 0 ? (
        <div
          style={{
            position: `absolute`,
            left: CX - dotSize / 2,
            top: CY - dotSize / 2,
            width: dotSize,
            height: dotSize,
            borderRadius: `50%`,
            background: C.green,
            boxShadow: `0 0 40px ${C.green}, 0 0 90px rgba(34,197,94,0.5)`,
            scale: String(dotS),
            opacity: dotO,
          }}
        />
      ) : null}
      <div
        style={{
          position: `absolute`,
          left: CX - lineW / 2,
          top: CY + 108,
          width: lineW,
          height: 1,
          opacity: lineO,
          background: `linear-gradient(to right, transparent, ${C.strokeActive} 20%, rgba(255,255,255,0.35) 50%, ${C.strokeActive} 80%, transparent)`,
        }}
      />
      <div
        style={{
          position: `absolute`,
          left: 0,
          right: 0,
          top: CY + 136,
          textAlign: `center`,
          fontFamily: UI,
          fontSize: 38,
          fontWeight: 500,
          letterSpacing: `-0.01em`,
          color: C.muted,
          clipPath: `inset(-20px ${(1 - tagIn) * 100}% -20px 0)`,
          translate: `0px ${14 * (1 - tagIn) - 18 * tagOut}px`,
          opacity: 1 - tagOut,
          filter: tagOut > 0.02 ? `blur(${10 * tagOut}px)` : undefined,
        }}
      >
        The next generation dev platform for teams
      </div>
    </AbsoluteFill>
  )
}
