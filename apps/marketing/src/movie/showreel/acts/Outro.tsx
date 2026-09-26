// acts/Outro.tsx — the loop closes (13.5–15 s): the corner chip springs back
// to the centre as the full lockup (the root's `Lockup` does the move), a
// green ring draws itself around the mark and settles into a hairline, and
// the site line lands underneath.

import React from "react"
import { AbsoluteFill } from "remotion"
import { C, MONO, UI } from "../theme"
import { enter, seg } from "../motion"
import { Ripple } from "../atoms"
import { LOCKUP_BIG } from "../lockup"

const RING_R = 112
const RING_AT = 10

export const Outro: React.FC<{ l: number }> = ({ l }) => {
  const cx = LOCKUP_BIG.x + LOCKUP_BIG.logo / 2
  const cy = LOCKUP_BIG.y
  const draw = seg(l, RING_AT, RING_AT + 22)
  const settle = seg(l, RING_AT + 24, RING_AT + 40)
  return (
    <AbsoluteFill>
      <svg style={{ position: `absolute`, inset: 0 }} width={1920} height={1080}>
        <circle
          cx={cx}
          cy={cy}
          r={RING_R}
          fill="none"
          stroke={C.green}
          strokeWidth={3 - settle * 1.5}
          pathLength={1}
          strokeDasharray={1}
          strokeDashoffset={1 - draw}
          strokeLinecap="round"
          opacity={1 - settle * 0.55}
          transform={`rotate(-90 ${cx} ${cy})`}
        />
      </svg>
      <Ripple f={l} at={RING_AT + 22} x={cx} y={cy} size={RING_R + 90} color={C.green} dur={24} />
      <div
        style={{
          position: `absolute`,
          left: 0,
          right: 0,
          top: cy + 140,
          textAlign: `center`,
          fontFamily: MONO,
          fontSize: 30,
          fontWeight: 500,
          letterSpacing: `0.04em`,
          color: C.text,
          ...enter(l, 16, 14, { rise: 18, blur: 8 }),
        }}
      >
        exponential.at
      </div>
      <div
        style={{
          position: `absolute`,
          left: 0,
          right: 0,
          top: cy + 192,
          textAlign: `center`,
          fontFamily: UI,
          fontSize: 22,
          fontWeight: 500,
          color: C.dim,
          ...enter(l, 21, 14, { rise: 14, blur: 6 }),
        }}
      >
        Automate software end to end · Open source · Your hardware
      </div>
    </AbsoluteFill>
  )
}
