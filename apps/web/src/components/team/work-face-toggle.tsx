import type { ReactNode } from "react"
import {
  conceptIcon,
  DropdownMenu,
  DropdownMenuTrigger,
  SEGMENTED_TAB,
  Tabs,
  TabsList,
  TabsTrigger,
} from "@exp/ui"
import { IssueRunMenuContent } from "@/components/issue-run-switcher"
import type { CodingSession } from "@/db/schema"
import type { PastRunRow } from "@/hooks/use-agents-data"
import { cn } from "@/lib/utils"
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
// lone `Runs` item still shows so the menu is never out of reach.

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
  /** `useIssueRuns` rows — the caret exists from two up. */
  runs: readonly PastRunRow[]
  checkedRunId?: string
  onOpen: (session: CodingSession) => void
}

const UiChevronDownIcon = conceptIcon(`ui-chevron-down`)

export function WorkFaceToggle({
  face,
  items,
  runMenu,
}: {
  face: WorkFace
  items: readonly WorkFaceItem[]
  runMenu?: WorkFaceRunMenu
}) {
  const hasRunMenu = (runMenu?.runs.length ?? 0) > 1
  if (items.length < 2 && !hasRunMenu) return null
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
        {items.map((item) =>
          item.face === `run` && runMenu && hasRunMenu ? (
            // The segment's capsule moves onto a wrapper so the label (the
            // tab) and the caret (the menu) are SIBLINGS inside one segment
            // — a button never nests in a button.
            <span
              key={item.face}
              data-state={face === `run` ? `active` : `inactive`}
              className="inline-flex h-[calc(100%-1px)] flex-1 items-center rounded-full border border-transparent text-foreground dark:text-muted-foreground dark:data-[state=active]:border-glass-stroke-active dark:data-[state=active]:bg-glass-active dark:data-[state=active]:text-foreground"
            >
              <TabsTrigger
                value={item.face}
                className={cn(
                  SEGMENTED_TAB,
                  `h-full border-0 pr-1 text-inherit dark:text-inherit dark:data-[state=active]:bg-transparent dark:data-[state=active]:text-inherit`
                )}
                data-face={item.face}
              >
                {item.label}
              </TabsTrigger>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <button
                    type="button"
                    className="inline-flex h-full items-center rounded-full pr-2.5 pl-0.5 opacity-70 outline-none hover:opacity-100 focus-visible:ring-[3px] focus-visible:ring-ring/50"
                    title="Switch run"
                    aria-label="Switch run"
                    data-testid="issue-run-switcher"
                  >
                    <UiChevronDownIcon className="size-3" />
                  </button>
                </DropdownMenuTrigger>
                <IssueRunMenuContent
                  runs={runMenu.runs}
                  checkedRunId={runMenu.checkedRunId}
                  onOpen={runMenu.onOpen}
                />
              </DropdownMenu>
            </span>
          ) : (
            <TabsTrigger
              key={item.face}
              value={item.face}
              className={SEGMENTED_TAB}
              data-face={item.face}
            >
              {item.label}
            </TabsTrigger>
          )
        )}
      </TabsList>
    </Tabs>
  )
}
