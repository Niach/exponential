import type { ReactNode } from "react"
import { additionsLabel, deletionsLabel } from "@exp/domain-contract/diff"

import { Button } from "./button"
import { cn } from "./cn"
import { DiffCounts } from "./diff-counts"
import { DropdownMenu, DropdownMenuTrigger } from "./dropdown-menu"
import { conceptIcon } from "./icons.generated"
import { SEGMENTED_TAB, Tabs, TabsList, TabsTrigger } from "./tabs"

// EXP-1152 — THE Work face strip, one component for every Work screen on the
// web: the md+ work header (`WorkFaceToggle`, EXP-870/877) and the phone's
// header band (`MobileFaceTabs`, EXP-1150). It is the segmented capsule
// (`tabs.tsx`) naming the faces `Issue · Run/Runs · +N −M · Results` in their
// fixed order — the desktop `work_header.rs` `FaceToggle`, iOS
// `WorkFaceTabs`, Android `FaceTabs.kt`. An unavailable face is HIDDEN, never
// disabled, and the strip is absent under two faces unless the `Runs` caret
// earns it (EXP-950: a lone `Runs` item keeps the run menu in reach). The
// menu's CONTENT is the app's (it reads live runs), so it is handed in; the
// strip owns only where the caret sits. A NULL face (EXP-1024, the workflow
// node panel) leaves every segment inactive.

/** The four faces. `diff` is the changes face, `results` the published
 *  screenshots (EXP-879). */
export type WorkFaceStripFace = `issue` | `run` | `diff` | `results`

export interface WorkFaceStripItem {
  face: WorkFaceStripFace
  label: ReactNode
  onSelect: () => void
}

const UiChevronDownIcon = conceptIcon(`ui-chevron-down`)

/** The word the Changes segment wears until its files are known. */
export const CHANGES_FACE_WORD = `Changes`

/** EXP-1152: the Changes segment's label — the desktop `FaceToggle::diff`
 *  rule on every client: the `+N −M` counts (`DiffCounts`, U+2212) once the
 *  face's files are known, the word `Changes` until then. The counts' one
 *  string is the segment's accessible name. */
export function ChangesFaceLabel({
  counts,
}: {
  counts: { additions: number; deletions: number } | null | undefined
}) {
  if (!counts) return <>{CHANGES_FACE_WORD}</>
  const text = `${additionsLabel(counts.additions)} ${deletionsLabel(counts.deletions)}`
  return (
    <DiffCounts
      additions={counts.additions}
      deletions={counts.deletions}
      title={text}
      aria-label={text}
    />
  )
}

export function WorkFaceStrip({
  face,
  items,
  runMenu,
  className,
}: {
  /** The face on show; `null` = none (every segment inactive). */
  face: WorkFaceStripFace | null
  items: readonly WorkFaceStripItem[]
  /** The `Runs` caret's menu — a `DropdownMenuContent` element. The caret
   *  shows only with this AND a `run` item to hang from. */
  runMenu?: { content: ReactNode }
  className?: string
}) {
  const hasRunMenu =
    runMenu !== undefined && items.some((item) => item.face === `run`)
  if (items.length < 2 && !hasRunMenu) return null
  return (
    <Tabs
      // Radix wants a string: a value no segment carries leaves them all
      // inactive, which is exactly what a null face means.
      value={face ?? ``}
      onValueChange={(next) => {
        if (next === face) return
        items.find((item) => item.face === next)?.onSelect()
      }}
      className={cn(`w-fit shrink-0`, className)}
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
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className="h-full w-auto rounded-full pr-2.5 pl-0.5 opacity-70 hover:bg-transparent hover:opacity-100 dark:hover:bg-transparent"
                    title="Switch run"
                    aria-label="Switch run"
                    data-testid="issue-run-switcher"
                  >
                    <UiChevronDownIcon className="size-3" />
                  </Button>
                </DropdownMenuTrigger>
                {runMenu.content}
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
