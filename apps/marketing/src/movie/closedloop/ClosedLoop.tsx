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
import { wallpaperBackground } from "../ships/rig"
import { Reel } from "./scenes/Reel"

// Static background: identical at every frame (no staticFile assets), so the
// loop point stays seamless and the @remotion/player embed needs no bundle.
// EXP-359: the blobs play the macOS wallpaper behind the translucent window —
// positioned so the bleed lands where the reference screenshot shows it
// (strong violet at the window's bottom-left corner, a softer wash along the
// right edge). ONE blob list shared with WindowChassis (rig.tsx), which
// paints the same wallpaper window-locally as the glass "backdrop".
const GradientBackground: React.FC = () => (
  <AbsoluteFill
    style={{ backgroundColor: C.canvas, backgroundImage: wallpaperBackground() }}
  />
)

export const ClosedLoop: React.FC<{ textScale?: number }> = ({
  textScale = 1,
}) => (
  <AbsoluteFill>
    <GradientBackground />
    <Reel textScale={textScale} />
  </AbsoluteFill>
)
