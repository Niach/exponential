// closedloop/chapters.ts — chapter metadata as a REMOTION-FREE module.
// The marketing site's SSR-rendered chapter rail (apps/marketing
// src/movie/LoopMovie.tsx) imports this file directly, so it must never
// grow imports; timeline.ts attaches the frame numbers and exports the
// full CHAPTERS list the player seeks by.

export type ChapterInfo = { id: string; label: string; phrase: string }

export const CHAPTER_INFO: ChapterInfo[] = [
  { id: "feedback", label: "Feedback", phrase: "a user reports a bug" },
  { id: "issue", label: "Issue", phrase: "lands on the board" },
  { id: "code", label: "Code", phrase: "Claude writes the fix" },
  { id: "merge", label: "Merge", phrase: "review, merge" },
  { id: "shipped", label: "Shipped", phrase: "the reporter hears back" },
]
