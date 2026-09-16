import {
  useEffect,
  useState,
  type ComponentProps,
  type MouseEvent,
  type ReactNode,
} from "react"
import { toast } from "sonner"
import {
  conceptIcon,
  Button,
  Pill,
  type buttonVariants,
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@exp/ui"
import { BUILTIN_FIX_CONFLICTS_ID } from "@/lib/builtin-actions"
import { mergeFailure, type MergeFailure } from "@/lib/merge-failure"
import { trpc } from "@/lib/trpc-client"
import { useOpenComposer } from "@/hooks/use-open-composer"
import type { VariantProps } from "class-variance-authority"

const PrMergedIcon = conceptIcon(`pr-merged`)
const UiLoadingIcon = conceptIcon(`ui-loading`)
const UiBranchIcon = conceptIcon(`ui-branch`)

// The session-scoped Merge control — the Agents list row (icon-only outline)
// and the steering view's glass pill (EXP-678) share it. Merge always closes
// (EXP-498): merges the PR, completes every linked issue, and ends the
// session server-side. Spinner held until the Electric echo flips the
// row's prState away from `open` (mirrors IssueMergeButton). Renders
// nothing unless there IS an open PR to merge; a batch session passes its
// resolved representative issue (EXP-535).
//
// EXP-734: the target is an issue XOR a session. An issue-LESS run (action or
// chat) that opened its own chore PR (EXP-626) carries prUrl/prNumber/prState
// on the SESSION row, so it merges through `codingSessions.mergePr` and
// settles on the session row's own echo. Recovery ("Fix conflicts") stays
// issue-only: the builtin action takes a representative ISSUE id, so a run
// PR's conflict just reports its refusal.
//
// EXP-706: when the merge is refused by a REAL conflict (EXP-533) and the
// caller wired the recovery run (`branch` + `teamId` + `steerEnabled`), this
// button REPLACES itself with "Fix conflicts" in the very same slot — the
// same swap the Reviews list and the review detail make. Every other refusal
// still reaches the user as a toast; nothing is swallowed.
//
// A refusal describes ONE snapshot of the pull request, so the swap is
// deliberately short-lived: a newer `updatedAt` (the Electric echo of a
// re-synced row) drops it, and while it stands a secondary "Retry merge"
// button keeps the plain merge one click away. Without both, a conflict
// resolved OUTSIDE the recovery run (a teammate rebases and pushes, GitHub
// recomputes mergeability) would hide Merge for the life of the open PR.

/** EXP-895: the two SHAPES the one merge control comes in. `pill` is the
 *  `Pill mode="action" primary` every Changes surface uses (the review's top
 *  bar, the run's, the phone capsule); `button` is the shadcn Button the list
 *  rows and the icon-only slots keep. */
export type MergeControlShape = `button` | `pill`

/** EXP-889: the pill arm's size. `md` = the Changes surfaces' capsule; `sm`
 *  = the property tray / run header, where it stands in a row of `sm`
 *  pills (status, priority, Stop) and must be the SAME box as its siblings. */
export type MergePillSize = `sm` | `md`

/** One control, either shape — so the merge logic below never branches twice. */
function MergeControl({
  as,
  pillSize = `md`,
  variant,
  size,
  className,
  disabled,
  ariaLabel,
  title,
  onClick,
  children,
}: {
  as: MergeControlShape
  pillSize?: MergePillSize
  variant?: VariantProps<typeof buttonVariants>[`variant`]
  size?: VariantProps<typeof buttonVariants>[`size`]
  className?: string
  disabled?: boolean
  ariaLabel: string
  title: string
  onClick: (e: MouseEvent) => void
  children: ReactNode
}) {
  if (as === `pill`) {
    return (
      <Pill
        size={pillSize}
        mode="action"
        primary
        className={className}
        disabled={disabled}
        aria-label={ariaLabel}
        title={title}
        onClick={onClick}
      >
        {children}
      </Pill>
    )
  }
  return (
    <Button
      variant={variant}
      size={size}
      className={className}
      disabled={disabled}
      aria-label={ariaLabel}
      title={title}
      onClick={onClick}
    >
      {children}
    </Button>
  )
}

export function SessionMergeButton({
  as = `button`,
  pillSize = `md`,
  prState,
  prNumber,
  issueId,
  sessionId,
  updatedAt,
  variant = `outline`,
  size = `icon`,
  className,
  label,
  branch,
  teamId,
  steerEnabled = false,
}: {
  /** EXP-895: `pill` = the primary glass capsule every Changes surface wears;
   *  `button` (the default) = the shadcn Button of the list rows. */
  as?: MergeControlShape
  /** EXP-889: the pill arm's size; `sm` beside the tray's `sm` pills. */
  pillSize?: MergePillSize
  prState: string | null
  prNumber: number | null
  /** The issue whose PR this merges. Pass this OR `sessionId`, never both. */
  issueId?: string
  /** EXP-734: the run whose OWN chore PR this merges (no linked issue). */
  sessionId?: string
  /**
   * The target row's `updated_at`. A new value means the row was re-synced, so
   * any stored refusal is about a stale snapshot and is dropped.
   */
  updatedAt?: string | Date | null
  variant?: VariantProps<typeof buttonVariants>[`variant`]
  size?: VariantProps<typeof buttonVariants>[`size`]
  className?: string
  /** Visible text beside the glyph; absent = icon-only. */
  label?: string
  /** The PR's branch — the recovery run rebases it, so it must be recorded. */
  branch?: string | null
  /** The team the recovery run belongs to. */
  teamId?: string | null
  /** Member + relay configured (`useSteerConfig`), resolved by the caller. */
  steerEnabled?: boolean
}) {
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [merging, setMerging] = useState(false)
  const [failure, setFailure] = useState<MergeFailure | null>(null)
  const stamp =
    updatedAt instanceof Date ? updatedAt.toISOString() : (updatedAt ?? null)

  useEffect(() => {
    if (prState !== `open`) {
      setMerging(false)
      setConfirmOpen(false)
      setFailure(null)
    }
  }, [prState])

  // A re-synced row supersedes the refusal captioned on the old one.
  // (A refused merge writes nothing server-side, so this never races its own
  // failure.)
  useEffect(() => {
    setFailure(null)
  }, [stamp])

  if (prState !== `open`) return null
  // The caller wired neither target — nothing to merge.
  if (!issueId && !sessionId) return null

  // Only a REAL conflict is fixable by the recovery run, and only where the
  // caller can actually launch one. The builtin action takes a representative
  // ISSUE id, so a run's own chore PR (EXP-734) never swaps.
  const canFixConflicts = Boolean(
    failure?.conflict && issueId && branch && teamId && steerEnabled
  )

  const merge = async () => {
    setMerging(true)
    setFailure(null)
    try {
      if (issueId) {
        await trpc.issues.mergePr.mutate(
          { issueId },
          { context: { skipErrorToast: true } }
        )
      } else if (sessionId) {
        await trpc.codingSessions.mergePr.mutate(
          { sessionId },
          { context: { skipErrorToast: true } }
        )
      }
      setConfirmOpen(false) // keep `merging` until the echo flips prState
    } catch (error) {
      const next = mergeFailure(
        error,
        `The pull request could not be merged`
      )
      setMerging(false)
      setConfirmOpen(false)
      setFailure(next)
      // The swap is this button's own caption for a conflict; every other
      // refusal has nowhere to live in a row this small, so it keeps the
      // global toast the link would otherwise have shown.
      if (!(next.conflict && issueId && branch && teamId && steerEnabled)) {
        toast.error(`Couldn't merge the pull request`, {
          description: next.message,
        })
      }
    }
  }

  const showFix = canFixConflicts && issueId && teamId

  return (
    <>
      {showFix ? (
        <>
          <FixConflictsButton
            as={as}
            pillSize={pillSize}
            issueId={issueId}
            variant={variant}
            size={size}
            className={className}
            label={label}
            message={failure?.message}
          />
          {/* The swap must never be a dead end: the conflict may have been
              resolved outside the recovery run, so Merge stays one click
              away as a quiet secondary. */}
          <Button
            variant="glass"
            size="icon-sm"
            disabled={merging}
            aria-label={merging ? `Merging…` : `Retry merge`}
            title={merging ? `Merging…` : `Retry merge`}
            onClick={(e) => {
              e.stopPropagation()
              setConfirmOpen(true)
            }}
          >
            {merging ? (
              <UiLoadingIcon className="animate-spin" />
            ) : (
              <PrMergedIcon />
            )}
          </Button>
        </>
      ) : (
        <MergeControl
          as={as}
          pillSize={pillSize}
          variant={variant}
          size={size}
          className={className}
          disabled={merging}
          ariaLabel={merging ? `Merging…` : `Merge pull request`}
          title={merging ? `Merging…` : `Merge`}
          onClick={(e) => {
            e.stopPropagation()
            setConfirmOpen(true)
          }}
        >
          {merging ? (
            <UiLoadingIcon className="animate-spin" />
          ) : (
            <PrMergedIcon />
          )}
          {label}
        </MergeControl>
      )}
      <Dialog
        open={confirmOpen}
        onOpenChange={(next) => {
          if (!merging) setConfirmOpen(next)
        }}
      >
        <DialogContent
          mobile="alert"
          className="sm:max-w-sm"
          onClick={(e) => e.stopPropagation()}
        >
          <DialogHeader>
            <DialogTitle>Merge pull request?</DialogTitle>
            <DialogDescription>
              {issueId
                ? `Merge PR #${prNumber ?? ``} into the default branch? Every issue linked to it completes, and its coding session closes.`
                : `Merge PR #${prNumber ?? ``} into the default branch? The run's coding session closes unless the team keeps sessions on merge.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel
              onClick={() => setConfirmOpen(false)}
              disabled={merging}
            />
            <Button onClick={merge} disabled={merging}>
              {merging ? (
                <UiLoadingIcon className="animate-spin" />
              ) : (
                <PrMergedIcon />
              )}
              Merge
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

// EXP-825: a navigation to the Agent page composer with the builtin picked
// and this PR pre-filled — no dialog, no device lookup here.
function FixConflictsButton({
  as,
  pillSize,
  issueId,
  variant,
  size,
  className,
  label,
  message,
}: {
  as: MergeControlShape
  pillSize?: MergePillSize
  issueId: string
  variant?: VariantProps<typeof buttonVariants>[`variant`]
  size?: VariantProps<typeof buttonVariants>[`size`]
  className?: string
  label?: string
  message?: string
}) {
  const openComposer = useOpenComposer()

  return (
    <MergeControl
      as={as}
      pillSize={pillSize}
      variant={variant}
      size={size}
      className={className}
      ariaLabel="Fix merge conflicts"
      title={message ?? `Fix merge conflicts`}
      onClick={(e) => {
        e.stopPropagation()
        openComposer({ actionId: BUILTIN_FIX_CONFLICTS_ID, prIssueId: issueId })
      }}
    >
      <UiBranchIcon />
      {label ? `Fix conflicts` : null}
    </MergeControl>
  )
}

/** EXP-895: THE merge control of a Changes surface — the primary glass pill,
 *  the same confirm/spinner/Fix-conflicts/Retry behaviour. Every Changes
 *  surface renders exactly ONE of these (the review's top bar, the run's, the
 *  phone work bar's capsule); it self-hides unless the PR is open. */
export function SessionMergePill(
  props: Omit<ComponentProps<typeof SessionMergeButton>, `as` | `variant` | `size`>
) {
  return <SessionMergeButton {...props} as="pill" />
}
