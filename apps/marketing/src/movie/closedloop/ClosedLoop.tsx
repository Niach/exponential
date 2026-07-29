// ClosedLoop — composition root. Five per-flow clips inside one stable frame
// (EXP-337): start coding → live steer → review & merge → live board →
// feedback intake, plus an END_HOLD rest tail (see timeline.ts).
// Player-compatible by construction: the background is a static CSS gradient
// (no staticFile assets), everything below is frame-driven. `textScale`
// (Player inputProps) scales ONLY the screen-space caption layer — the
// marketing embed passes 1.3 on phone widths so the narrative text stays
// readable at small sizes (EXP-176).

import React from "react"
import { AbsoluteFill } from "remotion"
import { C } from "../ships/theme"
import { Reel } from "./scenes/Reel"

// Static background: identical at every frame (no staticFile assets), so the
// loop point stays seamless and the @remotion/player embed needs no bundle.
const GradientBackground: React.FC = () => (
  <AbsoluteFill style={{ backgroundColor: C.canvas }}>
    <AbsoluteFill
      style={{
        background: `radial-gradient(720px 520px at 50% 32%, rgba(99,102,241,0.20), transparent 70%)`,
      }}
    />
    <AbsoluteFill
      style={{
        background: `radial-gradient(600px 400px at 88% 92%, rgba(129,140,248,0.10), transparent 70%)`,
      }}
    />
  </AbsoluteFill>
)

export const ClosedLoop: React.FC<{ textScale?: number }> = ({
  textScale = 1,
}) => (
  <AbsoluteFill>
    <GradientBackground />
    <Reel textScale={textScale} />
  </AbsoluteFill>
)
