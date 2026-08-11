// ClosedLoop — composition root. Five per-flow clips inside one stable frame
// (EXP-337): start coding → live steer → review & merge → live board →
// feedback intake, plus an END_HOLD rest tail (see timeline.ts).
// Player-compatible by construction: the background is a static CSS gradient
// (no staticFile assets), everything below is frame-driven. `portrait`
// (Player inputProps, true under viewport.ts SMALL_MEDIA) is the ONE phone
// signal the composition gets: under it the comp is 1080×1350 (EXP-482,
// replacing the EXP-392 landscape crops) and each clip swaps onto its
// portrait camera framing with the portrait caption treatment. One boolean —
// a single multiplier cannot express a per-clip crop, and two props saying
// "this is a phone" could disagree.

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
// paints the same wallpaper window-locally as the glass "backdrop". The blob
// field is authored on the 1920×1080 canvas; portrait shifts it to keep the
// same composition centered on the 1080×1350 one (EXP-482).
const GradientBackground: React.FC<{ portrait: boolean }> = ({ portrait }) => (
  <AbsoluteFill
    style={{
      backgroundColor: C.canvas,
      backgroundImage: portrait
        ? wallpaperBackground(-420, 135)
        : wallpaperBackground(),
    }}
  />
)

export const ClosedLoop: React.FC<{ portrait?: boolean }> = ({
  portrait = false,
}) => (
  <AbsoluteFill>
    <GradientBackground portrait={portrait} />
    <Reel portrait={portrait} />
  </AbsoluteFill>
)
