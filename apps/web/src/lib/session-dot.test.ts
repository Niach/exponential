import { describe, expect, it } from "vitest"
import { SESSION_DOT_CLASS } from "@/lib/session-dot"

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
})
