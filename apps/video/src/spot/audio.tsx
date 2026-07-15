// spot/audio.tsx — the §5 mix: music bed + keyboard foley. The footage clips
// carry their own diegetic audio (café → keystrokes → cup clink) with the duck
// baked into FootageClip; these two layers sit under/over that.
//
// music.mp3 (29.7s, measured 2026-07-13): intro 0–4s · build 8–16s · tension
// breakdown 17–21.5s · ARRIVAL at ~22s · natural fade from 27s. Anchored at
// f120 (the quiet hinge) the arrival lands at f≈774 — inside the "Shipped."
// punch (f750–790) — the loud tail carries the cup landing (f795+) and the
// fade dies into the brand card (f≈1011). Re-measure before swapping the track.

import React from "react"
import { Audio, Sequence, interpolate, staticFile } from "remotion"
import { KEYS_BED, MUSIC_IN } from "./timeline"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

export const SpotAudio: React.FC = () => (
  <>
    {/* music bed — enters as the café fades, owns the film from the UI act on */}
    <Sequence from={MUSIC_IN}>
      <Audio
        src={staticFile("footage/music.mp3")}
        volume={(f) =>
          interpolate(
            f,
            // local frames (global − MUSIC_IN): whisper under the typing beat,
            // half-open through the UI act, full for the arrival + payoff
            [0, 30, 108, 150, 600, 630],
            [0, 0.25, 0.25, 0.5, 0.5, 0.65],
            CLAMP,
          )
        }
      />
    </Sequence>

    {/* keyboard foley loop under the UI act — "the typing became the agent" */}
    <Sequence from={KEYS_BED.from} durationInFrames={KEYS_BED.to - KEYS_BED.from}>
      <Audio
        loop
        src={staticFile("footage/keys.mp3")}
        // barely-there tick under the music (trimmed down 2026-07-13 review r2)
        volume={(f) =>
          interpolate(f, [0, 30, KEYS_BED.to - KEYS_BED.from - 60, KEYS_BED.to - KEYS_BED.from], [0, 0.35, 0.35, 0], CLAMP)
        }
      />
    </Sequence>
  </>
)
