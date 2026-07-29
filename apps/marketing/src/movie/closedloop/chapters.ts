// closedloop/chapters.ts — per-flow clip metadata as a REMOTION-FREE module.
// The site's SSR-rendered stepper (../LoopMovie.tsx) imports this file
// directly, so it must never grow imports; timeline.ts attaches the frame
// numbers and exports the full CHAPTERS list the player seeks by.
//
// EXP-337: the movie is five self-contained flow clips inside one frame —
// IDE flows first (the IDE is the hero and the poster), collaboration
// fourth, intake last, so "feedback → issue lands on the board" wraps into
// "start coding" when the loop restarts.

export type FlowInfo = { id: string; label: string; phrase: string }

export const FLOW_INFO: FlowInfo[] = [
  {
    id: "start-coding",
    label: "Start coding",
    phrase: "one click starts an agent",
  },
  { id: "live-steer", label: "Live steer", phrase: "guide it from your phone" },
  { id: "review-merge", label: "Review & merge", phrase: "diff, review, ship" },
  { id: "board-live", label: "Live board", phrase: "your team in realtime" },
  { id: "feedback", label: "Feedback", phrase: "reports become issues" },
]
