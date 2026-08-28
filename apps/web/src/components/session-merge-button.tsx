import { useEffect, useState } from "react"
import { conceptIcon } from "@/lib/icons.generated"
import { trpc } from "@/lib/trpc-client"
import { Button, type buttonVariants } from "@/components/ui/button"
import type { VariantProps } from "class-variance-authority"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

const PrMergedIcon = conceptIcon(`pr-merged`)
const UiLoadingIcon = conceptIcon(`ui-loading`)

// The session-scoped Merge control — the Agents list row (icon-only outline)
// and the steering view's glass pill (EXP-678) share it. Merge always closes
// (EXP-498): merges the PR, completes every linked issue, and ends the
// session server-side. Spinner held until the Electric echo flips the
// issue's prState away from `open` (mirrors IssueMergeButton). Renders
// nothing unless there IS an open PR to merge; a batch session passes its
// resolved representative issue (EXP-535).
export function SessionMergeButton({
  prState,
  prNumber,
  issueId,
  variant = `outline`,
  size = `icon`,
  className,
  label,
}: {
  prState: string | null
  prNumber: number | null
  issueId: string
  variant?: VariantProps<typeof buttonVariants>[`variant`]
  size?: VariantProps<typeof buttonVariants>[`size`]
  className?: string
  /** Visible text beside the glyph; absent = icon-only. */
  label?: string
}) {
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [merging, setMerging] = useState(false)

  useEffect(() => {
    if (prState !== `open`) {
      setMerging(false)
      setConfirmOpen(false)
    }
  }, [prState])

  if (prState !== `open`) return null

  const merge = async () => {
    setMerging(true)
    try {
      // Failures surface via the global mutation-error toast.
      await trpc.issues.mergePr.mutate({ issueId })
      setConfirmOpen(false) // keep `merging` until the echo flips prState
    } catch {
      setMerging(false)
    }
  }

  return (
    <>
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
      <Dialog
        open={confirmOpen}
        onOpenChange={(next) => {
          if (!merging) setConfirmOpen(next)
        }}
      >
        <DialogContent
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
            <Button
              variant="ghost"
              onClick={() => setConfirmOpen(false)}
              disabled={merging}
            >
              Cancel
            </Button>
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
