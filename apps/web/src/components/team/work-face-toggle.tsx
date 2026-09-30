import type { ReactNode } from "react"
import { WorkFaceStrip } from "@exp/ui"
import { IssueRunMenuContent } from "@/components/issue-run-switcher"
import type { CodingSession } from "@/db/schema"
import type { PastRunRow } from "@/hooks/use-agents-data"
import type { WorkTabFace } from "@/lib/work-tabs"

// EXP-870: an issue and its run are ONE work tab with faces. This is the
// segmented control the unified work header carries (desktop `work_header.rs`
// top row) — the same segmented pill as the list nav's Inbox / My Issues
// strip. EXP-877: the faces are `Issue` (issue-bound), `Run` (a run exists)
// and the diff (`@exp/ui` `DiffCounts` — `+N −M`, once the run has changes; the
// label lives there since EXP-895, with the rest of the diff vocabulary);
// EXP-879 adds `results`, the run's published screenshots, last in the strip.
// An unavailable face is HIDDEN, never disabled, and the control itself is
// absent under two faces — unless (EXP-950) the issue has several runs of
// mine: then the `Runs` segment carries a caret opening the run menu, and a
// lone `Runs` item still shows so the menu is never out of reach. EXP-974:
// an issue-less run's menu lists its RESUME CHAIN (`lib/sessions/run-chain`),
// so a resumed run and its successor wear the same toggle and pick each
// other from the caret instead of a band above the transcript. EXP-1024: a
// NULL face leaves every segment inactive — the workflow node panel, where
// the reader is on the graph and each segment is a way OUT of it rather than
// a picture of where they are.

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
 *  caret beside the plural (EXP-950) picks between them. */
export const RUNS_FACE_LABEL = `Runs`
/** EXP-879: the run's published screenshots. The segment is a word like
 *  `Issue` and `Run` — the diff segment's counts are the exception. */
export const RESULTS_FACE_LABEL = `Results`

export function runFaceLabel(multipleRuns: boolean): string {
  return multipleRuns ? RUNS_FACE_LABEL : RUN_FACE_LABEL
}

/** EXP-950: the runs behind the `Runs` segment's caret. */
export interface WorkFaceRunMenu {
  /** `useIssueRuns` rows (an issue-less run: `useRunChain`, EXP-974) — the
   *  caret exists from two up. */
  runs: readonly PastRunRow[]
  checkedRunId?: string
  onOpen: (session: CodingSession) => void
}

export function WorkFaceToggle({
  face,
  items,
  runMenu,
}: {
  /** The face on show; `null` = none (every segment inactive). */
  face: WorkFace | null
  items: readonly WorkFaceItem[]
  runMenu?: WorkFaceRunMenu
}) {
  // EXP-1152: the strip itself is `@exp/ui` `WorkFaceStrip` (the phone's
  // `MobileFaceTabs` and the styleguide wear the same one); this plugs in the
  // app's run menu. The caret needs SEVERAL runs to pick between.
  return (
    <WorkFaceStrip
      face={face}
      items={items}
      runMenu={
        runMenu && runMenu.runs.length > 1
          ? {
              content: (
                <IssueRunMenuContent
                  runs={runMenu.runs}
                  checkedRunId={runMenu.checkedRunId}
                  onOpen={runMenu.onOpen}
                />
              ),
            }
          : undefined
      }
    />
  )
}
