// ClosedLoop — composition root. A seamless-looping ~26s product story:
// feedback in → PR out → reporter hears back. 1920×1080 @ 30fps, 780 frames.
// Player-compatible by construction: the background is a static CSS gradient
// (no staticFile assets), everything below is frame-driven.

import React from "react"
import { AbsoluteFill } from "remotion"
import { C } from "../ships/theme"
import { Film } from "./scenes/Film"

// Static twin of src/components.tsx Background (which the marketing player
// must not import — that module pulls staticFile-based footage components).
// No drift: identical at every frame, so the loop point stays seamless.
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

export const ClosedLoop: React.FC = () => (
  <AbsoluteFill>
    <GradientBackground />
    <Film />
  </AbsoluteFill>
)
