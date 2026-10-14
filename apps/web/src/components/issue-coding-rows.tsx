import { useState } from "react"
import { conceptIcon, FabButton, Pill } from "@exp/ui"
import type { Board, Issue } from "@/db/schema"
import { cn } from "@/lib/utils"
import { useOpenComposer } from "@/hooks/use-open-composer"
import { useCodingReadiness } from "@/hooks/use-coding-readiness"
import {
  CodingReadinessOverlay,
  ReadinessCaption,
  ReadinessStartPill,
} from "@/components/coding-readiness-checklist"

// EXP-1293 (Threads v2): the issue page's ONE coding control is Start: the
// white pill in the md+ header's trailing slot, the right circle of the
// phone bar. Ready, it opens the home composer seeded with this issue (the
// send lands a new thread); a missing step (GitHub, the board's repository,
// a device online) turns it into the readiness checklist's entry point
// (EXP-1121). No Stop, no Merge, no PR row: a live session is the thread's,
// a pull request the Guide's.

// The play glyph the Actions surfaces run with.
const ActionRunIcon = conceptIcon(`action-run`)

/** The issue page's Start label (the composer it opens says the rest). */
export const ISSUE_START_LABEL = `Start`

/** PR-state pill: open emerald / merged purple / closed rose / draft plain. */
export function PrStateBadge({ state }: { state: string | null | undefined }) {
  if (!state) return null
  if (state === `draft`) return <Pill>Draft</Pill>
  const styles: Record<string, string> = {
    open: `text-emerald-400`,
    merged: `text-purple-400`,
    closed: `text-rose-400`,
  }
  const cls = styles[state]
  if (!cls) return null
  return <Pill className={cn(`capitalize`, cls)}>{state}</Pill>
}

/** `pill` = the md+ header's Start pill, `fab` = the phone bar's circle. */
export type IssueStartVariant = `pill` | `fab`

/**
 * Start ALWAYS renders for a member on an instance with remote start: the
 * primary pill once the issue can start right now, else the dashed amber
 * one captioned with the FIRST missing step. Either opens the "Ready to
 * code?" checklist until ready; ready, it opens the composer with this
 * issue as its subject.
 */
export function IssueStartButton({
  issue,
  board,
  teamId,
  currentUserId,
  variant,
}: {
  issue: Pick<Issue, `id`>
  board: Pick<Board, `id`>
  teamId: string
  currentUserId: string
  variant: IssueStartVariant
}) {
  const state = useCodingReadiness({
    teamId,
    boardIds: [board.id],
    currentUserId,
  })
  const { readiness } = state
  const openComposer = useOpenComposer()
  const [open, setOpen] = useState(false)

  // Non-member, or no remote start on this instance: nothing to offer.
  if (!readiness.visible) return null

  const start = () => openComposer({ issueIds: [issue.id] })
  const press = () => {
    if (readiness.loading) return
    if (readiness.ready) start()
    else setOpen((value) => !value)
  }
  const notReady = !readiness.loading && !readiness.ready

  if (variant === `fab`) {
    // EXP-1121: the play circle ALWAYS stays on a phone, dashed with an
    // amber badge dot while a step is missing; a tap opens the checklist
    // as a bottom sheet.
    return (
      <CodingReadinessOverlay
        state={state}
        open={open}
        onOpenChange={setOpen}
        onStart={start}
        surface="sheet"
      >
        <FabButton
          emphasis="primary"
          aria-label={ISSUE_START_LABEL}
          aria-disabled={readiness.loading || undefined}
          data-testid="issue-start"
          data-readiness={
            readiness.loading ? `loading` : readiness.ready ? `ready` : `missing`
          }
          className={cn(
            `relative`,
            notReady && `border-dashed border-amber-400/60 text-foreground/70`
          )}
          onClick={press}
        >
          <ActionRunIcon className="size-5" />
          {notReady && (
            <span
              aria-hidden
              data-testid="coding-readiness-badge"
              className="absolute top-2.5 right-2.5 size-2 rounded-full bg-amber-400 ring-2 ring-background"
            />
          )}
        </FabButton>
      </CodingReadinessOverlay>
    )
  }

  return (
    <CodingReadinessOverlay
      state={state}
      open={open}
      onOpenChange={setOpen}
      onStart={start}
      surface="popover"
    >
      {readiness.caption && (
        <ReadinessCaption caption={readiness.caption} onClick={press} />
      )}
      <ReadinessStartPill
        readiness={readiness}
        tone="primary"
        label={ISSUE_START_LABEL}
        icon={<ActionRunIcon />}
        onClick={press}
        testId="issue-start"
      />
    </CodingReadinessOverlay>
  )
}
