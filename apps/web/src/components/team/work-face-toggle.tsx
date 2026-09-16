import type { ReactNode } from "react"
import { SEGMENTED_TAB, Tabs, TabsList, TabsTrigger } from "@exp/ui"
import type { WorkTabFace } from "@/lib/work-tabs"

// EXP-870: an issue and its run are ONE work tab with faces. This is the
// segmented control the unified work header carries (desktop `work_header.rs`
// top row) — the same segmented pill as the list nav's Inbox / My Issues
// strip. EXP-877: the faces are `Issue` (issue-bound), `Run` (a run exists)
// and the diff (`@exp/ui` `DiffCounts` — `+N −M`, once the run has changes; the
// label lives there since EXP-895, with the rest of the diff vocabulary);
// EXP-879 adds `results`, the run's published screenshots, last in the strip.
// An unavailable face is HIDDEN, never disabled, and the control itself is
// absent under two faces.

/** The four faces a work tab can show. `diff` is the run's changes,
 *  `results` its published screenshots (EXP-879). */
export type WorkFace = WorkTabFace | `diff` | `results`

export interface WorkFaceItem {
  face: WorkFace
  label: ReactNode
  onSelect: () => void
}

/** Byte-identical with the IDE. */
export const ISSUE_FACE_LABEL = `Issue`
export const RUN_FACE_LABEL = `Run`
/** EXP-886: the Run face's label once the issue has MORE THAN ONE run of
 *  mine (`selectIssueRuns`). The segment still opens the tab's run; the
 *  plural says the session view has a switcher between them. */
export const RUNS_FACE_LABEL = `Runs`
/** EXP-879: the run's published screenshots. The segment is a word like
 *  `Issue` and `Run` — the diff segment's counts are the exception. */
export const RESULTS_FACE_LABEL = `Results`

export function runFaceLabel(multipleRuns: boolean): string {
  return multipleRuns ? RUNS_FACE_LABEL : RUN_FACE_LABEL
}

export function WorkFaceToggle({
  face,
  items,
}: {
  face: WorkFace
  items: readonly WorkFaceItem[]
}) {
  if (items.length < 2) return null
  return (
    <Tabs
      value={face}
      onValueChange={(next) => {
        if (next === face) return
        items.find((item) => item.face === next)?.onSelect()
      }}
      className="w-fit shrink-0"
      data-testid="work-face-toggle"
    >
      <TabsList>
        {items.map((item) => (
          <TabsTrigger
            key={item.face}
            value={item.face}
            className={SEGMENTED_TAB}
            data-face={item.face}
          >
            {item.label}
          </TabsTrigger>
        ))}
      </TabsList>
    </Tabs>
  )
}
