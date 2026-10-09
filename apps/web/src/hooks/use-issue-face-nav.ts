import { guideSearch, type GuideSectionKey } from "@/lib/work-faces"

// EXP-1251: the session route's search when a Work screen opens a RUN: the
// list origin rides along, and a Guide target (`?view=guide`, a section page
// and its file in focus) when the run's own Guide is asked for. Pure, so the
// routes' wiring stays testable without mounting them. EXP-1251 dropped the
// issue's live-diff hand-off: the issue's Guide draws the run's live diff
// itself.

export interface RunFaceTarget {
  from?: string
  /** Open the run on its Guide (else the Run face). */
  guide?: boolean
  section?: GuideSectionKey
  file?: string | null
}

export function runFaceSearch(input: RunFaceTarget): {
  from?: string
  view?: `guide`
  section?: GuideSectionKey
  file?: string
} {
  if (!input.guide) return input.from ? { from: input.from } : {}
  return guideSearch(input)
}
