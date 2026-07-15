// spot-vertical/audio.tsx — the 15s mix. Music runs from f0 but trimmed 13.4s
// into the track so the measured ARRIVAL (~22s in-source) lands under the
// "Shipped." card (f≈258); the natural fade dies inside the outro. Keys bed
// ticks under the single UI beat. Footage clips carry their own diegetic audio.

import React from "react"
import { Audio, Sequence, interpolate, staticFile } from "remotion"
import { KEYS_BED_V, MUSIC_TRIM_SEC } from "./timeline"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

export const SpotAudioV: React.FC = () => (
  <>
    <Audio
      src={staticFile("footage/music.mp3")}
      trimBefore={Math.round(MUSIC_TRIM_SEC * 30)}
      // under the café at first, half-open through the UI beat, full for the
      // arrival + payoff (mirrors the 16:9 ratios on the compressed clock)
      volume={(f) => interpolate(f, [0, 20, 120, 150, 240, 258], [0, 0.3, 0.3, 0.5, 0.5, 0.65], CLAMP)}
    />
    <Sequence from={KEYS_BED_V.from} durationInFrames={KEYS_BED_V.to - KEYS_BED_V.from}>
      <Audio
        loop
        src={staticFile("footage/keys.mp3")}
        volume={(f) =>
          interpolate(
            f,
            [0, 20, KEYS_BED_V.to - KEYS_BED_V.from - 40, KEYS_BED_V.to - KEYS_BED_V.from],
            [0, 0.35, 0.35, 0],
            CLAMP,
          )
        }
      />
    </Sequence>
  </>
)
