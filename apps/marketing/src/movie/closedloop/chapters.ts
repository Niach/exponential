// closedloop/chapters.ts — per-flow clip metadata as a REMOTION-FREE module.
// The site's SSR-rendered stepper (../LoopMovie.tsx) imports this file
// directly, so it must never grow imports; timeline.ts attaches the frame
// numbers and exports the full CHAPTERS list the player seeks by.
//
// EXP-385: the film opens on the multiplayer board (teams managing their
// product is the headline), then the merged remote-start + live-steer clip,
// review/merge/deploy, intake, and the platform-lineup finale. "Feedback →
// issue lands on the board" still wraps into the live board when the loop
// restarts. Labels only — the per-flow subtitle phrases were cut (EXP-388).

export type FlowInfo = { id: string; label: string }

export const FLOW_INFO: FlowInfo[] = [
  { id: "board-live", label: "Live board" },
  { id: "code-everywhere", label: "Code from everywhere" },
  { id: "review-merge", label: "Review & merge" },
  { id: "feedback", label: "Feedback" },
  { id: "platforms", label: "Every platform" },
]
