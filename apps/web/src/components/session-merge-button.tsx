import { useEffect, useState } from "react"
import { toast } from "sonner"
import { conceptIcon } from "@/lib/icons.generated"
import { BUILTIN_FIX_CONFLICTS_ID } from "@/lib/builtin-actions"
import { mergeFailure, type MergeFailure } from "@/lib/merge-failure"
import { trpc } from "@/lib/trpc-client"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { LaunchDialog } from "@/components/launch-dialog/launch-dialog"
import { Button, type buttonVariants } from "@/components/ui/button"
import type { VariantProps } from "class-variance-authority"
import {
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

const PrMergedIcon = conceptIcon(`pr-merged`)
const UiLoadingIcon = conceptIcon(`ui-loading`)
const UiBranchIcon = conceptIcon(`ui-branch`)

// The session-scoped Merge control — the Agents list row (icon-only outline)
// and the steering view's glass pill (EXP-678) share it. Merge always closes
// (EXP-498): merges the PR, completes every linked issue, and ends the
// session server-side. Spinner held until the Electric echo flips the
// issue's prState away from `open` (mirrors IssueMergeButton). Renders
// nothing unless there IS an open PR to merge; a batch session passes its
// resolved representative issue (EXP-535).
//
// EXP-706: when the merge is refused by a REAL conflict (EXP-533) and the
// caller wired the recovery run (`branch` + `teamId` + `steerEnabled`), this
// button REPLACES itself with "Fix conflicts" in the very same slot — the
// same swap the Reviews list and the review detail make. Every other refusal
// still reaches the user as a toast; nothing is swallowed.
//
// A refusal describes ONE snapshot of the pull request, so the swap is
// deliberately short-lived: a newer `issueUpdatedAt` (the Electric echo of a
// re-synced row) drops it, and while it stands a secondary "Retry merge"
// button keeps the plain merge one click away. Without both, a conflict
// resolved OUTSIDE the recovery run (a teammate rebases and pushes, GitHub
// recomputes mergeability) would hide Merge for the life of the open PR.
export function SessionMergeButton({
  prState,
  prNumber,
  issueId,
  issueUpdatedAt,
  variant = `outline`,
  size = `icon`,
  className,
  label,
  branch,
  teamId,
  currentUserId,
  steerEnabled = false,
}: {
  prState: string | null
  prNumber: number | null
  issueId: string
  /**
   * The issue row's `updated_at`. A new value means the row was re-synced, so
   * any stored refusal is about a stale snapshot and is dropped.
   */
  issueUpdatedAt?: string | Date | null
  variant?: VariantProps<typeof buttonVariants>[`variant`]
  size?: VariantProps<typeof buttonVariants>[`size`]
  className?: string
  /** Visible text beside the glyph; absent = icon-only. */
  label?: string
  /** The PR's branch — the recovery run rebases it, so it must be recorded. */
  branch?: string | null
  /** The team the recovery run belongs to. */
  teamId?: string | null
  /** Keys the launcher's device list to the caller's own machines. */
  currentUserId?: string
  /** Member + relay configured (`useSteerConfig`), resolved by the caller. */
  steerEnabled?: boolean
}) {
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [merging, setMerging] = useState(false)
  const [failure, setFailure] = useState<MergeFailure | null>(null)
  const stamp =
    issueUpdatedAt instanceof Date
      ? issueUpdatedAt.toISOString()
      : (issueUpdatedAt ?? null)

  useEffect(() => {
    if (prState !== `open`) {
      setMerging(false)
      setConfirmOpen(false)
      setFailure(null)
    }
  }, [prState])

  // A re-synced issue row supersedes the refusal captioned on the old one.
  // (A refused merge writes nothing server-side, so this never races its own
  // failure.)
  useEffect(() => {
    setFailure(null)
  }, [stamp])

  if (prState !== `open`) return null

  // Only a REAL conflict is fixable by the recovery run, and only where the
  // caller can actually launch one.
  const canFixConflicts = Boolean(
    failure?.conflict && branch && teamId && steerEnabled
  )

  const merge = async () => {
    setMerging(true)
    setFailure(null)
    try {
      await trpc.issues.mergePr.mutate(
        { issueId },
        { context: { skipErrorToast: true } }
      )
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
      if (!(next.conflict && branch && teamId && steerEnabled)) {
        toast.error(`Couldn't merge the pull request`, {
          description: next.message,
        })
      }
    }
  }

  const showFix = canFixConflicts && teamId

  return (
    <>
      {showFix ? (
        <>
          <FixConflictsButton
            issueId={issueId}
            teamId={teamId}
            currentUserId={currentUserId}
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
            variant="ghost"
            size="icon"
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
        <Button
          variant={variant}
          size={size}
          className={className}
          disabled={merging}
          aria-label={merging ? `Merging…` : `Merge pull request`}
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
        </Button>
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
              {`Merge PR #${prNumber ?? ``} into the default branch? Every issue linked to it completes, and its coding session closes.`}
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

// Mounted ONLY once a conflict has been seen — the device lookup behind
// `useRemoteStart` must not run for every idle Merge button on the page.
function FixConflictsButton({
  issueId,
  teamId,
  currentUserId,
  variant,
  size,
  className,
  label,
  message,
}: {
  issueId: string
  teamId: string
  currentUserId?: string
  variant?: VariantProps<typeof buttonVariants>[`variant`]
  size?: VariantProps<typeof buttonVariants>[`size`]
  className?: string
  label?: string
  message?: string
}) {
  const [open, setOpen] = useState(false)
  const remote = useRemoteStart({ enabled: true, currentUserId, teamId })

  return (
    <>
      <Button
        variant={variant}
        size={size}
        className={className}
        aria-label="Fix merge conflicts"
        title={message ?? `Fix merge conflicts`}
        onClick={(e) => {
          e.stopPropagation()
          setOpen(true)
        }}
      >
        <UiBranchIcon />
        {label ? `Fix conflicts` : null}
      </Button>
      {open && (
        <LaunchDialog
          open
          onOpenChange={(next) => {
            if (!next) setOpen(false)
          }}
          devices={remote.devices ?? []}
          starting={remote.starting}
          teamId={teamId}
          initialTab="actions"
          initialActionId={BUILTIN_FIX_CONFLICTS_ID}
          initialPrIssueId={issueId}
          onStartIssues={(device, options, issueIds) => {
            remote
              .startIssues(device, options, issueIds)
              .then(() => setOpen(false))
              .catch(() => {})
          }}
          onRunAction={(device, action, options, inputs) => {
            remote
              .runAction(device, action, options, inputs)
              .then(() => setOpen(false))
              .catch(() => {})
          }}
        />
      )}
    </>
  )
}
