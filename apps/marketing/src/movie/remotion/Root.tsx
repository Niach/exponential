// The Remotion studio/render root. The site itself never imports this file —
// it embeds the composition directly through @remotion/player
// (../LoopMoviePlayer); this root exists so `bun run movie:studio` and
// `movie:render` can preview and render the SAME ClosedLoop component.
import "./reset.css"
import React from "react"
import { Composition } from "remotion"
import { ClosedLoop, DURATION_IN_FRAMES, FPS } from "../closedloop"

export const RemotionRoot: React.FC = () => (
  <Composition
    id="ClosedLoop"
    component={ClosedLoop}
    durationInFrames={DURATION_IN_FRAMES}
    fps={FPS}
    width={1920}
    height={1080}
  />
)
