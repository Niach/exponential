// LaunchSpot — composition root: the 36s launch ad (storyboard-launch-spot.md).
// Seedance live-action + recut UI beats (f0–990), hard cut to the brand card
// (f990–1080). 1920×1080 @ 30fps, 1080 frames.

import React from "react"
import { AbsoluteFill, Sequence } from "remotion"
import { Background } from "../components"
import { Outro } from "../ships/scenes/Outro"
import { SpotAudio } from "./audio"
import { Spot } from "./Spot"
import { SEG, SPOT_COPY } from "./timeline"

export const LaunchSpot: React.FC = () => (
  <AbsoluteFill>
    <Background />
    <SpotAudio />
    <Sequence from={0} durationInFrames={SEG.filmEnd}>
      <Spot />
    </Sequence>
    <Sequence from={SEG.filmEnd} durationInFrames={SEG.outroEnd - SEG.filmEnd}>
      {/* no fadeOutAt — the 90f sequence never reaches the default 112, so the
          brand card + URL hold to the last frame (CTA dwell ≈1.9s, was 0.9s) */}
      <Outro tagline={SPOT_COPY.tagline} urlIn={30} />
    </Sequence>
  </AbsoluteFill>
)
