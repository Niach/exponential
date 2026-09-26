// showreel/Showreel.tsx — the 15-second Exponential showreel (EXP-1100).
// Render-only (studio + `movie:showreel:render`); the site never embeds it.
// Six acts on one drifting wallpaper (timeline.ts), the brand lockup living
// through all of them (lockup.tsx), a progress hairline closing the loop
// along the bottom edge. Every act is authored at local frame 0.
//
// DOM order is the compositing order at the two overlaps that need it: the
// clients act sits BELOW the agents act so the agents' slanted wipe reveals
// it, and the ship act sits above the scattering clients.

import React from "react"
import { AbsoluteFill, useCurrentFrame } from "remotion"
import { wallpaperBackground } from "../ships/rig"
import { C, UI, VIOLET, W } from "./theme"
import { seg } from "./motion"
import { useShowreelFonts } from "./fonts"
import { ACTS, DURATION_IN_FRAMES, type Act } from "./timeline"
import { Lockup } from "./lockup"
import { Ignition } from "./acts/Ignition"
import { Board } from "./acts/Board"
import { Agents } from "./acts/Agents"
import { Everywhere } from "./acts/Everywhere"
import { Ship } from "./acts/Ship"
import { Outro } from "./acts/Outro"

export { DURATION_IN_FRAMES, FPS } from "./timeline"

const Backdrop: React.FC<{ f: number }> = ({ f }) => {
  const dx = Math.sin(f / 110) * 40
  const dy = Math.cos(f / 140) * 30
  const wash = seg(f, 0, 56) * 0.9
  return (
    <AbsoluteFill style={{ backgroundColor: C.canvas }}>
      <AbsoluteFill style={{ backgroundImage: wallpaperBackground(dx, dy), opacity: wash }} />
      <AbsoluteFill
        style={{
          backgroundImage: `radial-gradient(circle, rgba(255,255,255,0.09) 1px, transparent 1.6px)`,
          backgroundSize: `36px 36px`,
          backgroundPosition: `${dx * 0.4}px ${dy * 0.4}px`,
          WebkitMaskImage: `radial-gradient(ellipse 58% 56% at 50% 50%, black, transparent)`,
          maskImage: `radial-gradient(ellipse 58% 56% at 50% 50%, black, transparent)`,
          opacity: wash * 0.8,
        }}
      />
      <AbsoluteFill
        style={{
          background: `radial-gradient(ellipse 85% 75% at 50% 50%, transparent 45%, rgba(0,0,0,0.65))`,
        }}
      />
    </AbsoluteFill>
  )
}

const Progress: React.FC<{ f: number }> = ({ f }) => (
  <div
    style={{
      position: `absolute`,
      left: 0,
      bottom: 0,
      height: 3,
      width: (f / DURATION_IN_FRAMES) * W,
      background: `linear-gradient(to right, ${VIOLET}, ${C.green})`,
      opacity: seg(f, 24, 48) * 0.75,
      boxShadow: `0 0 12px rgba(139,92,246,0.6)`,
    }}
  />
)

export const Showreel: React.FC = () => {
  useShowreelFonts()
  const f = useCurrentFrame()
  const inAct = (a: Act) => f >= a.from && f < a.to

  return (
    <AbsoluteFill style={{ backgroundColor: C.canvas, overflow: `hidden`, fontFamily: UI, color: C.text }}>
      <Backdrop f={f} />
      {inAct(ACTS.ignition) ? <Ignition f={f} /> : null}
      {inAct(ACTS.board) ? <Board l={f - ACTS.board.from} /> : null}
      {inAct(ACTS.everywhere) ? <Everywhere l={f - ACTS.everywhere.from} /> : null}
      {inAct(ACTS.agents) ? <Agents l={f - ACTS.agents.from} /> : null}
      {inAct(ACTS.ship) ? <Ship l={f - ACTS.ship.from} /> : null}
      {inAct(ACTS.outro) ? <Outro l={f - ACTS.outro.from} /> : null}
      {f >= 8 ? <Lockup f={f} /> : null}
      <Progress f={f} />
    </AbsoluteFill>
  )
}
