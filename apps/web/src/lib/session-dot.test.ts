import { describe, expect, it } from "vitest"
import { LIVE_DOT_TONE, SESSION_DOT_CLASS } from "@exp/ui"
import { LIVE_DOT_TONE_BY_SESSION_TONE } from "@/components/agent-session-row"

// EXP-862: the mapping is hand-mirrored on the desktop
// (`queries::session_dot_tone`) and both natives, so it is locked here — a
// tone changes in four places or not at all.
describe(`the session dot`, () => {
  it(`pins one colour per tone`, () => {
    expect(SESSION_DOT_CLASS).toEqual({
      running: `bg-emerald-500`,
      review: `bg-emerald-500`,
      needs_input: `bg-amber-500`,
      done: `bg-sky-500`,
      muted: `bg-muted-foreground/40`,
    })
  })

  // EXP-887: the session rows and the work-tab strip draw the dot through the
  // `LiveDot` primitive instead of pasting the disc markup, so the primitive's
  // tone has to paint EXACTLY what the ×4 table says. Without this the two
  // tables could drift apart silently.
  it(`maps every session tone onto a LiveDot tone of the same colour`, () => {
    for (const [tone, cls] of Object.entries(SESSION_DOT_CLASS)) {
      const mapped =
        LIVE_DOT_TONE_BY_SESSION_TONE[
          tone as keyof typeof SESSION_DOT_CLASS
        ]
      expect(LIVE_DOT_TONE[mapped].core, `${tone} → ${mapped}`).toBe(cls)
    }
  })
})
