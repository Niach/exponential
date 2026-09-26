// showreel/lockup.tsx — the brand lockup that lives through the whole reel
// (EXP-1100). It is born at the centre from the ignition dot (the mark draws
// its cut curves while the wordmark's letters land one by one), collapses
// into the top-left chip for the four product acts, and springs back to the
// centre for the outro. ONE element, one continuous transform: `t` = 0 is
// the big centred lockup, 1 the chip.

import React from "react"
import { ExpLogo } from "../ships/rig"
import { C, DISPLAY } from "./theme"
import { SETTLE, lerp, mix, pop, seg } from "./motion"
import { Kinetic } from "./atoms"
import { LOCKUP_RETURN, LOCKUP_TO_CHIP } from "./timeline"

const LOGO = 150
const GAP = 32
const TEXT_W = 756
const GROUP_W = LOGO + GAP + TEXT_W

export const LOCKUP_BIG = { x: (1920 - GROUP_W) / 2, y: 540, logo: LOGO } as const
const CHIP = { x: 64, y: 56, scale: 28 / LOGO } as const

export const lockupT = (f: number): number => {
  const back = pop(f, LOCKUP_RETURN, SETTLE)
  return seg(f, LOCKUP_TO_CHIP, LOCKUP_TO_CHIP + 14) * (1 - back)
}

export const Lockup: React.FC<{ f: number }> = ({ f }) => {
  const t = lockupT(f)
  const x = lerp(LOCKUP_BIG.x, CHIP.x, t)
  const y = lerp(LOCKUP_BIG.y, CHIP.y, t)
  const s = lerp(1, CHIP.scale, t)

  // Birth: the mark appears at the screen centre (where the dot was) and
  // slides into its slot as the letters arrive.
  const logoShift = mix(f, 16, 40, 1920 / 2 - (LOCKUP_BIG.x + LOGO / 2), 0)
  const logoScale = 0.5 + 0.5 * pop(f, 8, SETTLE)
  const drawT = seg(f, 10, 38)
  const discO = seg(f, 8, 16)
  const tracking = mix(f, 18, 56, 0.06, -0.035)

  return (
    <div
      style={{
        position: `absolute`,
        left: x,
        top: y - LOGO / 2,
        width: GROUP_W,
        height: LOGO,
        transformOrigin: `0 50%`,
        transform: `scale(${s})`,
        display: `flex`,
        alignItems: `center`,
        gap: GAP,
        zIndex: 50,
      }}
    >
      <div
        style={{
          width: LOGO,
          height: LOGO,
          flexShrink: 0,
          translate: `${logoShift}px 0px`,
          scale: String(logoScale),
          filter: `drop-shadow(0 0 ${40 * (1 - t)}px rgba(255,255,255,0.18))`,
        }}
      >
        <ExpLogo size={LOGO} drawT={drawT} discO={discO} />
      </div>
      <div
        style={{
          width: TEXT_W,
          fontFamily: DISPLAY,
          fontSize: 140,
          fontWeight: 600,
          letterSpacing: `${tracking}em`,
          color: C.text,
          lineHeight: 1,
          whiteSpace: `nowrap`,
        }}
      >
        <Kinetic text="Exponential" f={f} at={18} step={2} dur={18} rise={56} blur={16} />
      </div>
    </div>
  )
}
