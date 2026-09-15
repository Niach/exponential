import { useState, type ReactNode } from "react"
import type { Board, Issue } from "@/db/schema"
import {
  conceptIcon,
  Button,
  PILL_PRIMARY_PAINT,
  type SessionDotTone,
} from "@exp/ui"
import { cn } from "@/lib/utils"
import { useReviewFiles } from "@/hooks/use-review-files"
import { useSteerConfig } from "@/components/agent-session"
import { ChangesFileSheet } from "@/components/changes-file-sheet"
import { ChangesView } from "@/components/changes-view"
import { IssueMobileHeader } from "@/components/issue-mobile-header"
import {
  MOBILE_WORK_BAR_CLEARANCE,
  MOBILE_WORK_CAPSULE_CLASS,
  MOBILE_WORK_CIRCLE_CLASS,
  MobileWorkBar,
} from "@/components/mobile-work-bar"
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

const GithubIcon = conceptIcon(`ui-github`)
const UiLoadingIcon = conceptIcon(`ui-loading`)

/** The GitHub control of a Changes surface — the PR page in a new tab. The
 *  phone wears it in the HEADER's action slot; the work bar's leading slot is
 *  the file sheet's (EXP-895). */
export function GithubGhostButton({ prUrl }: { prUrl: string }) {
  return (
    <Button
      variant="ghost"
      size="icon"
      className="size-9 shrink-0 rounded-full text-muted-foreground hover:text-foreground"
      aria-label="Open pull request on GitHub"
      title="Open PR on GitHub"
      data-testid="changes-github-action"
      onClick={() => window.open(prUrl, `_blank`, `noopener,noreferrer`)}
    >
      <GithubIcon className="size-4" />
    </Button>
  )
}

/** The GitHub circle of a phone work bar — kept for the surfaces that still put
 *  it in a bar slot (the run's Changes face has no issue header to hang it on
 *  when the run is issue-less). */
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

/** The Changes face's Merge PR capsule: the 52px work capsule painted with
 *  `Pill`'s own `primary` accent (which has to come LAST — the capsule brings
 *  its own glass fill), carrying the two-click confirm and the Fix-conflicts
 *  swap `SessionMergePill` already has. It self-hides unless the PR is open. */
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
    <SessionMergePill
      {...props}
      label={MERGE_PR_LABEL}
      className={cn(
        MOBILE_WORK_CAPSULE_CLASS,
        `justify-center rounded-full`,
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
          issue.prUrl ? <GithubGhostButton prUrl={issue.prUrl} /> : undefined
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
             alone here (`nav="none"`) and every one of them starts closed. */
          <ChangesView
            files={files}
            nav="none"
            defaultCollapsed
            selected={selected}
            onSelect={setSelected}
            emptyLabel="No changes in this pull request."
          />
        )}
      </div>
      <MobileWorkBar
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
