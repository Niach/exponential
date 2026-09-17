import { useState, type ReactNode } from "react"
import type { Board, Issue } from "@/db/schema"
import {
  conceptIcon,
  PILL_PRIMARY_PAINT,
  type SessionDotTone,
} from "@exp/ui"
import { cn } from "@/lib/utils"
import { PrGithubButton } from "@/components/pr-github-button"
import { useReviewFiles } from "@/hooks/use-review-files"
import { useSteerConfig } from "@/components/agent-session"
import { ChangesFileSheet } from "@/components/changes-file-sheet"
import { ChangesView } from "@/components/changes-view"
import { IssueMobileHeader } from "@/components/issue-mobile-header"
import {
  MOBILE_WORK_BAR_CLEARANCE,
  MOBILE_WORK_CAPSULE_CLASS,
  MobileWorkBar,
} from "@/components/mobile-work-bar"
import { PrGraphBadge } from "@/components/pr-graph-badge"
import { MERGE_PR_LABEL } from "@/components/run-action-pills"
import { SessionMergePill } from "@/components/session-merge-button"
import { useIssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"

// EXP-893: the Changes FACE of an issue subject with NO shown run — the
// issue has an open PR (or a pushed branch), so its files are the face
// (`useReviewFiles`, the review route's own loader). Same header as the
// Issue face, the diff in the column, and the bar: the file SHEET on the left
// (EXP-895 — GitHub moved up into the header's action slot, where a phone
// header has room for it), the Merge PR capsule in the centre while the PR is
// open, the face switcher on the right. A run's live diff draws the same face
// inside the session view.

const UiLoadingIcon = conceptIcon(`ui-loading`)

/** The Changes face's Merge PR capsule: the 52px work capsule painted with
 *  `Pill`'s own `primary` accent (which has to come LAST — the capsule brings
 *  its own glass fill), carrying the two-click confirm and the Fix-conflicts
 *  swap `SessionMergePill` already has. It self-hides unless the PR is open. */
/** EXP-916: the phone's ONE Merge control — a SOLID white pill hugging its
 *  label (28px padding, a 20px glyph) in the bar's centred cluster, on the
 *  Reviews page and the Work screen's Changes face alike. */
export function MergeCapsule(props: {
  issueId?: string
  sessionId?: string
  prState: string | null
  prNumber: number | null
  branch: string | null
  updatedAt: string | Date | null
  steerEnabled: boolean
}) {
  return (
    <SessionMergePill
      {...props}
      label={MERGE_PR_LABEL}
      className={cn(
        MOBILE_WORK_CAPSULE_CLASS,
        `flex-none justify-center rounded-full px-7 font-medium [&_svg]:size-5`,
        PILL_PRIMARY_PAINT
      )}
    />
  )
}

export function IssueChangesFace({
  issue,
  board,
  teamSlug,
  teamId,
  readOnly,
  origin,
  switcher,
  dot,
}: {
  issue: Issue
  board: Board
  teamSlug: string
  teamId: string
  readOnly: boolean
  origin?: string
  /** The face switcher circle. */
  switcher: ReactNode
  dot?: { tone: SessionDotTone; connecting?: boolean } | null
}) {
  const { state } = useReviewFiles(issue)
  const [selected, setSelected] = useState<string | null>(null)
  // The `…` menu's Move to board / Unmark duplicate, the same handlers the
  // issue face binds (`use-issue-property-handlers.ts`).
  const handlers = useIssuePropertyHandlers({ issue, teamSlug, readOnly })
  const steerConfig = useSteerConfig()
  const steerEnabled = Boolean(steerConfig?.enabled) && !readOnly
  const files = state.kind === `files` ? state.files : []

  return (
    <div className="flex h-full min-h-0 flex-col" data-testid="issue-changes-face">
      <IssueMobileHeader
        issue={issue}
        board={board}
        teamSlug={teamSlug}
        teamId={teamId}
        readOnly={readOnly}
        origin={origin}
        handlers={handlers}
        action={
          issue.prUrl ? <PrGithubButton prUrl={issue.prUrl} /> : undefined
        }
        graphBadge={
          /* EXP-897: the Changes face's own overlay — the PR stack bottom-up,
             with `Merge stack` on its bottom entry. */
          <PrGraphBadge
            teamId={teamId}
            teamSlug={teamSlug}
            face="changes"
            issue={issue}
          />
        }
        dot={dot}
      />
      <div
        className={cn(
          `min-h-0 flex-1 overflow-y-auto overscroll-contain bg-card/40`,
          MOBILE_WORK_BAR_CLEARANCE
        )}
      >
        {state.kind === `loading` && (
          <div className="flex items-center gap-2 px-4 py-6 text-sm text-muted-foreground">
            <UiLoadingIcon className="size-4 animate-spin" />
            Loading changes…
          </div>
        )}
        {state.kind === `none` && (
          <p className="px-4 py-6 text-sm text-muted-foreground">
            No changes yet — nothing has been pushed for this issue.
          </p>
        )}
        {state.kind === `error` && (
          <p className="px-4 py-6 text-sm text-destructive">{state.message}</p>
        )}
        {state.kind === `files` && (
          /* The file LIST is the bar's sheet on a phone, so the cards stand
             alone here (`nav="none"`). EXP-916: they start OPEN, like every
             other diff surface — only a file past the contract's collapse
             threshold folds itself. */
          <ChangesView
            files={files}
            nav="none"
            selected={selected}
            onSelect={setSelected}
            emptyLabel="No changes in this pull request."
          />
        )}
      </div>
      <MobileWorkBar
        /* EXP-916: the Reviews page's cluster — files · Merge PR · switcher. */
        cluster
        leading={
          files.length > 0 ? (
            <ChangesFileSheet
              files={files}
              selected={selected}
              onSelect={setSelected}
            />
          ) : undefined
        }
        capsule={
          issue.prState === `open` ? (
            <MergeCapsule
              issueId={issue.id}
              prState={issue.prState}
              prNumber={issue.prNumber}
              branch={issue.branch}
              updatedAt={issue.updatedAt}
              steerEnabled={steerEnabled}
            />
          ) : undefined
        }
        trailing={switcher}
      />
      {handlers.duplicatePicker}
    </div>
  )
}
