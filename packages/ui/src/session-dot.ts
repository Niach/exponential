// EXP-862: the ONE session-dot mapping, hand-mirrored with the desktop's
// `queries::session_dot_tone` and the two natives — a running or
// ready-for-review run is emerald, one that waits on a human (needs input, or
// gone quiet past the stale threshold) amber, a done one sky (the
// issue-status palette's blue), an ended or paused one muted. Every web
// surface that draws a session dot reads THIS table: the session rows
// (`components/agent-session-row.tsx`) and the session header's `PhaseDot`.
//
// A plain lib module on purpose: the two components that draw the dot sit on
// opposite sides of an existing import cycle.

export const SESSION_DOT_CLASS = {
  running: `bg-emerald-500`,
  review: `bg-emerald-500`,
  needs_input: `bg-amber-500`,
  done: `bg-sky-500`,
  muted: `bg-muted-foreground/40`,
} as const

export type SessionDotTone = keyof typeof SESSION_DOT_CLASS
