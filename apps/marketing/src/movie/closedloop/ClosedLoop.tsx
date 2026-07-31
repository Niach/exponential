// ClosedLoop — composition root. Five per-flow clips inside one stable frame
// (EXP-337): start coding → live steer → review & merge → live board →
// feedback intake, plus an END_HOLD rest tail (see timeline.ts).
// Player-compatible by construction: the background is a static CSS gradient
// (no staticFile assets), everything below is frame-driven. `small` (Player
// inputProps, true under viewport.ts SMALL_MEDIA) is the ONE phone signal the
// composition gets: it enlarges the screen-space caption layer (EXP-176) and
// swaps each clip onto its tight mobile camera framing (EXP-392). It replaced
// the old numeric `textScale` — a single multiplier cannot express a per-clip
// crop, and two props saying "this is a phone" could disagree.

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

export const ClosedLoop: React.FC<{ small?: boolean }> = ({
  small = false,
}) => (
  <AbsoluteFill>
    <GradientBackground />
    <Reel small={small} />
  </AbsoluteFill>
)
