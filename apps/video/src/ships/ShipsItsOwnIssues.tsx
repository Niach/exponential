// ShipsItsOwnIssues — composition root: canvas Background (shared drifting-glow
// canvas), the continuous Film (f0–1380), then a hard cut to the Outro brand
// card (f1380–1500). 1920×1080 @ 30fps, 1500 frames.

import React from "react"
import { AbsoluteFill, Sequence } from "remotion"
import { Background } from "../components"
import { Film } from "./scenes/Film"
import { Outro } from "./scenes/Outro"
import { SCENE } from "./scenes/timeline"

export const ShipsItsOwnIssues: React.FC = () => (
  <AbsoluteFill>
    <Background />
    <Sequence from={0} durationInFrames={SCENE.filmEnd}>
      <Film />
    </Sequence>
    <Sequence from={SCENE.filmEnd} durationInFrames={SCENE.outroEnd - SCENE.filmEnd}>
      <Outro />
    </Sequence>
  </AbsoluteFill>
)
