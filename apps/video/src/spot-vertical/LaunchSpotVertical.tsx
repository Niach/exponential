// LaunchSpotVertical — composition root: the 15s Instagram/Reels cut.
// 1080×1920 @30fps, 450 frames. Film body f0–378, brand card f378–450
// (holds to the last frame — CTA dwell over 2s on the compressed clock).

import React from "react"
import { AbsoluteFill, Sequence } from "remotion"
import { Background } from "../components"
import { Outro } from "../ships/scenes/Outro"
import { SpotAudioV } from "./audio"
import { SpotVertical } from "./SpotVertical"
import { COPY_V, SEGV } from "./timeline"

export const LaunchSpotVertical: React.FC = () => (
  <AbsoluteFill>
    <Background />
    <SpotAudioV />
    <Sequence from={0} durationInFrames={SEGV.outro}>
      <SpotVertical />
    </Sequence>
    <Sequence from={SEGV.outro} durationInFrames={SEGV.end - SEGV.outro}>
      <Outro tagline={COPY_V.tagline} urlIn={26} />
    </Sequence>
  </AbsoluteFill>
)
