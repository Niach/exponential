import { RING_TONE_CLASS } from "@exp/ui"
import { severity } from "@/lib/agent-usage"

// EXP-877: the composer footer's CONTEXT RING — the 16px radial that replaced
// the context pill. It is the same number the pill drew (`contextPercent`,
// lib/agent-usage.ts) and the same three tones every usage surface uses
// (`severity`: amber from 75%, destructive from 95%) — only the shape is new,
// so no threshold and no percent rule is restated here.
//
// EXP-961: the geometry and the three tone CLASSES moved into `@exp/ui`
// beside the component that draws them; what stays here is the app's binding
// of the shared `severity` thresholds onto those tones.

export {
  RING_SIZE,
  RING_STROKE,
  RING_RADIUS,
  ringGeometry,
  type RingGeometry,
} from "@exp/ui"

/** The ring's colour — muted while there is room, amber at the warning
 *  threshold, destructive at the danger one (`severity`). */
export function ringToneClass(percent: number): string {
  return RING_TONE_CLASS[severity(percent)]
}
