import type { ReactNode } from "react"

import { AgentRunMark, type RunMarkState } from "./agent-brand-mark"
import { Button } from "./button"
import { cn } from "./cn"
import { DropdownMenu, DropdownMenuTrigger } from "./dropdown-menu"
import { conceptIcon } from "./icons.generated"
import { SESSION_DOT_CLASS, type SessionDotTone } from "./session-dot"
import { SEGMENTED_TAB, Tabs, TabsList, TabsTrigger } from "./tabs"

// EXP-1152 — THE Work face strip, one component for every Work screen on the
// web: the md+ work header (`WorkFaceToggle`, EXP-870/877) and the phone's
// header band (`MobileFaceTabs`, EXP-1150). It is the segmented capsule
// (`tabs.tsx`) naming the faces `Issue · Run/Runs · Guide` in their fixed
// order (EXP-1251: Changes + Results merged into the Guide; its counts moved
// into the body) — the desktop `work_header.rs` `FaceToggle`, iOS
// `WorkFaceTabs`, Android `FaceTabs.kt`. An unavailable face is HIDDEN, never
// disabled, and the strip is absent under two faces unless the `Runs` caret
// earns it (EXP-950: a lone `Runs` item keeps the run menu in reach). The
// menu's CONTENT is the app's (it reads live runs), so it is handed in; the
// strip owns only where the caret sits. A NULL face (EXP-1024, the workflow
// node panel) leaves every segment inactive.

/** The three faces (EXP-1251). */
export type WorkFaceStripFace = `issue` | `run` | `guide`

export interface WorkFaceStripItem {
  face: WorkFaceStripFace
  label: ReactNode
  onSelect: () => void
}

const UiChevronDownIcon = conceptIcon(`ui-chevron-down`)

/** EXP-1162: the segments' state (contract `detail-chrome.json` FACE MARKS,
 *  ×4) — the header title carries none. The Run tab never draws a dot: while
 *  its run is live it wears the agent's brand mark (`AgentRunMark`, the
 *  sidebar Running row's), leading the label, with the amber badge while it
 *  waits on a person. An open pull request puts a 6px dot on the Guide tab,
 *  6px after the label. */
export type WorkFaceStripDots = Partial<
  Record<WorkFaceStripFace, SessionDotTone>
>

const FACE_DOT_LABEL: Partial<Record<SessionDotTone, string>> = {
  running: `Running`,
  needs_input: `Needs input`,
  review: `Pull request open`,
}

/** The run behind the Run tab: whose mark it wears, and what it is doing
 *  (EXP-1184, the ×4 `session-display.json` rule — the working spark, or a
 *  state badge). */
export interface WorkFaceStripRun {
  agent: string | null | undefined
  state?: RunMarkState
}

const RUN_STATE_LABEL: Record<RunMarkState, string> = {
  working: `Working`,
  needs_input: `Needs input`,
  review: `Pull request open`,
  done: `Done`,
  ended: `Ended`,
}

function RunMark({
  tone,
  run,
}: {
  tone: SessionDotTone | undefined
  run: WorkFaceStripRun | undefined
}) {
  if (!tone) return null
  const state =
    run?.state ?? (tone === `needs_input` ? `needs_input` : undefined)
  const label = state ? RUN_STATE_LABEL[state] : FACE_DOT_LABEL[tone]
  return (
    <span
      role="img"
      aria-label={label}
      title={label}
      data-testid="face-run-mark"
      data-tone={tone}
      data-state={state}
      // The segment's own `gap-1.5` is the contract's 6px.
      className="inline-flex shrink-0"
    >
      <AgentRunMark
        agent={run?.agent}
        state={state}
        ringClassName="ring-background"
      />
    </span>
  )
}

function FaceDot({ tone }: { tone: SessionDotTone | undefined }) {
  if (!tone) return null
  return (
    <span
      role="img"
      aria-label={FACE_DOT_LABEL[tone]}
      title={FACE_DOT_LABEL[tone]}
      data-testid="face-dot"
      data-tone={tone}
      className={cn(
        `inline-block size-1.5 shrink-0 rounded-full`,
        SESSION_DOT_CLASS[tone]
      )}
    />
  )
}

export function WorkFaceStrip({
  face,
  items,
  runMenu,
  dots,
  run,
  className,
}: {
  /** The face on show; `null` = none (every segment inactive). */
  face: WorkFaceStripFace | null
  items: readonly WorkFaceStripItem[]
  /** The `Runs` caret's menu — a `DropdownMenuContent` element. The caret
   *  shows only with this AND a `run` item to hang from. */
  runMenu?: { content: ReactNode }
  /** EXP-1162: the segments' state, by face. */
  dots?: WorkFaceStripDots
  /** EXP-1162: the run whose mark the Run tab wears. */
  run?: WorkFaceStripRun
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
                <RunMark tone={dots?.run} run={run} />
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
              {item.face === `run` && <RunMark tone={dots?.run} run={run} />}
              {item.label}
              {item.face !== `run` && <FaceDot tone={dots?.[item.face]} />}
            </TabsTrigger>
          )
        )}
      </TabsList>
    </Tabs>
  )
}
