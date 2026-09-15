import { useState, type ReactNode } from "react"
import type { Board, Issue } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { cn } from "@/lib/utils"
import { useReviewFiles } from "@/hooks/use-review-files"
import { useSteerConfig } from "@/components/agent-session"
import { IssueMobileHeader } from "@/components/issue-mobile-header"
import {
  MOBILE_WORK_BAR_CLEARANCE,
  MOBILE_WORK_CAPSULE_CLASS,
  MOBILE_WORK_CIRCLE_CLASS,
  MobileWorkBar,
} from "@/components/mobile-work-bar"
import { MERGE_PILL_CLASS, MERGE_PR_LABEL } from "@/components/run-action-pills"
import { SessionDiffFace } from "@/components/session-diff-face"
import { SessionMergeButton } from "@/components/session-merge-button"
import type { SessionDotTone } from "@/lib/session-dot"
import { useIssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"

// EXP-893: the Changes FACE of an issue subject with NO shown run — the
// issue has an open PR (or a pushed branch), so its files are the face
// (`useReviewFiles`, the review route's own loader). Same header as the
// Issue face, the diff in the column, and the bar: GitHub on the left, the
// Merge PR capsule in the centre while the PR is open, the face switcher on
// the right. A run's live diff draws the same face inside the session view.

const GithubIcon = conceptIcon(`ui-github`)
const UiLoadingIcon = conceptIcon(`ui-loading`)

/** The GitHub circle of the Changes face's bar — the PR page in a new tab. */
export function GithubCircle({ prUrl }: { prUrl: string }) {
  return (
    <button
      type="button"
      aria-label="Open pull request on GitHub"
      title="Open PR on GitHub"
      data-testid="changes-github-circle"
      onClick={() => window.open(prUrl, `_blank`, `noopener,noreferrer`)}
      className={MOBILE_WORK_CIRCLE_CLASS}
    >
      <GithubIcon className="size-5" />
    </button>
  )
}

/** The Changes face's Merge PR capsule: the primary glass capsule with the
 *  two-click confirm and the Fix-conflicts swap `SessionMergeButton` carries.
 *  It self-hides unless the PR is open. */
export function MergeCapsule(props: {
  issueId?: string
  sessionId?: string
  prState: string | null
  prNumber: number | null
  branch: string | null
  teamId: string | null
  updatedAt: string | Date | null
  steerEnabled: boolean
}) {
  return (
    <SessionMergeButton
      {...props}
      variant="glass"
      size="lg"
      label={MERGE_PR_LABEL}
      className={cn(
        MOBILE_WORK_CAPSULE_CLASS,
        MERGE_PILL_CLASS,
        `justify-center rounded-full`
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
        dot={dot}
      />
      <div
        className={cn(
          `min-h-0 flex-1 overflow-y-auto overscroll-contain bg-card/40 px-4 py-3`,
          MOBILE_WORK_BAR_CLEARANCE
        )}
      >
        {state.kind === `loading` && (
          <div className="flex items-center gap-2 py-6 text-sm text-muted-foreground">
            <UiLoadingIcon className="size-4 animate-spin" />
            Loading changes…
          </div>
        )}
        {state.kind === `none` && (
          <p className="py-6 text-sm text-muted-foreground">
            No changes yet — nothing has been pushed for this issue.
          </p>
        )}
        {state.kind === `error` && (
          <p className="py-6 text-sm text-destructive">{state.message}</p>
        )}
        {state.kind === `files` && state.files.length === 0 && (
          <p className="py-6 text-sm text-muted-foreground">
            No changes in this pull request.
          </p>
        )}
        {state.kind === `files` && state.files.length > 0 && (
          /* `SessionDiffFace`'s file nav already heads the list with the
             count and the +/- totals — the summary row the natives draw. */
          <SessionDiffFace
            files={state.files}
            selected={selected}
            onSelect={setSelected}
          />
        )}
      </div>
      <MobileWorkBar
        leading={issue.prUrl ? <GithubCircle prUrl={issue.prUrl} /> : undefined}
        capsule={
          issue.prState === `open` ? (
            <MergeCapsule
              issueId={issue.id}
              prState={issue.prState}
              prNumber={issue.prNumber}
              branch={issue.branch}
              teamId={issue.teamId}
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
