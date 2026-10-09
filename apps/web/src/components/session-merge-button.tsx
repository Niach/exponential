import {
  useEffect,
  useState,
  type ComponentProps,
  type MouseEvent,
  type ReactNode,
} from "react"
import {
  mergeIssuePrPrompt,
  mergeRunPrPrompt,
  promptActions,
} from "@/lib/prompts"
import {
  conceptIcon,
  Button,
  FabButton,
  Pill,
  type buttonVariants,
  Prompt,
  toast,
} from "@exp/ui"
import { BUILTIN_FIX_CONFLICTS_ID } from "@/lib/builtin-actions"
import { mergeFailure, type MergeFailure } from "@/lib/merge-failure"
import type { StackMergeConfirm } from "@/lib/pr-stack"
import { trpc } from "@/lib/trpc-client"
import { useOpenComposer } from "@/hooks/use-open-composer"
import { useStackMergeConfirm } from "@/hooks/use-stack-merge-choice"
import { useLinkedIssueCount } from "@/hooks/use-linked-issue-count"
import {
  StackMergeConfirmDialog,
  type StackMergeInput,
} from "@/components/stack-merge-choice-dialog"
import type { VariantProps } from "class-variance-authority"

/** EXP-917/EXP-1233: the ONE gate on the recovery run — a REAL conflict
 *  (EXP-533), on an ISSUE target (the builtin action takes a representative
 *  issue, so a run's own chore PR never qualifies), with a recorded branch
 *  (the run rebases it) and the relay configured. When it holds, the refused
 *  merge OPENS the composer on the Fix merge conflicts builtin at once; when
 *  it does not, the refusal is a toast. Pure, so the rule is a test; mirrors
 *  Android `canOfferFixConflicts`, iOS `canFixConflicts` and desktop
 *  `work_header::conflict_opens_composer`. */
export function canOfferFixConflicts({
  failure,
  issueId,
  branch,
  steerEnabled,
}: {
  failure: MergeFailure | null | undefined
  issueId: string | null | undefined
  branch: string | null | undefined
  steerEnabled: boolean
}): boolean {
  return Boolean(failure?.conflict && issueId && branch && steerEnabled)
}

const PrMergedIcon = conceptIcon(`pr-merged`)
const UiLoadingIcon = conceptIcon(`ui-loading`)

// The session-scoped Merge control — the Agents list row (icon-only outline)
// and the steering view's glass pill (EXP-678) share it. Merge always closes
// (EXP-498): merges the PR, completes every linked issue, and ends the
// session server-side. Spinner held until the Electric echo flips the
// row's prState away from `open` (mirrors IssueMergeButton). Renders
// nothing unless there IS an open PR to merge.
//
// EXP-734: the target is an issue XOR a session. An issue-LESS run (batch,
// action or chat) that opened its own PR carries prUrl/prNumber/prState on
// the SESSION row, so it merges through `codingSessions.mergePr` and
// settles on the session row's own echo. Recovery stays issue-only: the
// builtin action takes a representative ISSUE id, so a run PR's conflict
// just reports its refusal.
//
// EXP-1233: when the merge is refused by a REAL conflict (EXP-533) and the
// caller wired the recovery run (`branch` + `steerEnabled`), the refusal
// OPENS the launcher on the Fix merge conflicts builtin with this pull
// request picked and the conflict flagged (the composer's card says why it
// is up) — no toast, and no "Fix conflicts" button parked in the Merge slot
// (EXP-706's swap, with its "Retry merge" secondary and its `updatedAt`
// expiry, is gone: the button is plain Merge again the moment the dialog is
// up, so a conflict resolved outside the run is one click away). Every
// other refusal still reaches the user as a toast; nothing is swallowed.
//
// EXP-917: the gate takes NOTHING from the caller that a synced issue row
// cannot supply (no `teamId` — the board-scoped `issues` shape drops it,
// REV2-5; the composer resolves the team from the route).
//
// EXP-1248: a PR that is a member of an open linear STACK never merges off
// the plain confirm (the server refuses a plain merge of any member). The
// click first reads the stack off the synced rows (`useStackMergeConfirm`,
// armed by the click so a list of these buttons costs no live queries) and
// opens `StackMergeConfirmDialog`: "Merge stack" lands the whole open chain
// through its top. A run's own chore PR is in no stack.

/** EXP-895: the two SHAPES the one merge control comes in. `pill` is the
 *  `Pill mode="action" primary` every Changes surface uses (the review's top
 *  bar, the run's, the phone capsule); `button` is the shadcn Button the list
 *  rows and the icon-only slots keep. EXP-1191: `fab` = the phone Work bar's
 *  52px glass circle (`FabButton`), the merge glyph alone. */
export type MergeControlShape = `button` | `pill` | `fab`

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
  if (as === `fab`) {
    return (
      <FabButton
        emphasis="primary"
        className={className}
        disabled={disabled}
        aria-label={ariaLabel}
        title={title}
        onClick={onClick}
      >
        {children}
      </FabButton>
    )
  }
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
  variant = `outline`,
  size = `icon`,
  className,
  label,
  branch,
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
  variant?: VariantProps<typeof buttonVariants>[`variant`]
  size?: VariantProps<typeof buttonVariants>[`size`]
  className?: string
  /** Visible text beside the glyph; absent = icon-only. */
  label?: string
  /** The PR's branch — the recovery run rebases it, so it must be recorded. */
  branch?: string | null
  /** Member + relay configured (`useSteerConfig`), resolved by the caller. */
  steerEnabled?: boolean
}) {
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [merging, setMerging] = useState(false)
  const openComposer = useOpenComposer()
  // EXP-1248: the click ARMS the stack read; the answer decides which dialog
  // opens. `stackConfirm` holds the stack confirm's content while it is open.
  const [armed, setArmed] = useState(false)
  const [stackConfirm, setStackConfirm] = useState<StackMergeConfirm | null>(
    null
  )
  const stack = useStackMergeConfirm(issueId, armed)
  // Armed with the stack read, so a batch PR's body counts its issues on the
  // confirm's first paint instead of flipping after it opens.
  const linkedCount = useLinkedIssueCount(issueId, armed || confirmOpen)
  const mergeCopy = issueId
    ? mergeIssuePrPrompt({ number: prNumber, count: linkedCount })
    : mergeRunPrPrompt(prNumber)

  useEffect(() => {
    if (prState !== `open`) {
      setMerging(false)
      setConfirmOpen(false)
      setArmed(false)
      setStackConfirm(null)
    }
  }, [prState])

  // The armed click resolves as soon as the rows are in: a stack member opens
  // the stack dialog, anything else the plain confirm.
  useEffect(() => {
    if (!armed || !stack.ready) return
    setArmed(false)
    if (stack.confirm) setStackConfirm(stack.confirm)
    else setConfirmOpen(true)
  }, [armed, stack.ready, stack.confirm])

  if (prState !== `open`) return null
  // The caller wired neither target — nothing to merge.
  if (!issueId && !sessionId) return null

  // The click itself: a session PR goes straight to the plain confirm, an
  // issue PR first asks the synced rows about its stack.
  const arm = () => {
    if (issueId) setArmed(true)
    else setConfirmOpen(true)
  }

  const merge = async () => {
    setMerging(true)
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
      setStackConfirm(null)
    } catch (error) {
      const next = mergeFailure(
        error,
        `The pull request could not be merged`
      )
      setMerging(false)
      setConfirmOpen(false)
      setStackConfirm(null)
      // EXP-1233: a real conflict on an issue PR opens the recovery run's
      // composer at once, this pull request picked and the refusal flagged
      // (EXP-825: a navigation/dialog seed, no device lookup here). Every
      // other refusal has nowhere to live in a row this small, so it keeps
      // the global toast.
      if (
        issueId &&
        canOfferFixConflicts({ failure: next, issueId, branch, steerEnabled })
      ) {
        openComposer({
          actionId: BUILTIN_FIX_CONFLICTS_ID,
          prIssueId: issueId,
          conflict: true,
        })
      } else {
        toast.error(`Couldn't merge the pull request`, {
          description: next.message,
        })
      }
    }
  }

  // EXP-1248: a stack merge lands several pull requests in one GitHub
  // merge-async; its refusal is about the stack, never a rebase-and-resolve
  // job for THIS one, so it is toasted, never a run.
  const mergeStack = async (input: StackMergeInput) => {
    setMerging(true)
    try {
      await trpc.issues.mergePr.mutate(input, {
        context: { skipErrorToast: true },
      })
      setStackConfirm(null) // keep `merging` until the echo flips prState
    } catch (error) {
      const next = mergeFailure(error, `The stack could not be merged`)
      setMerging(false)
      setStackConfirm(null)
      toast.error(`Couldn't merge the stack`, { description: next.message })
    }
  }

  return (
    <>
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
          arm()
        }}
      >
        {merging ? (
          <UiLoadingIcon className="animate-spin" />
        ) : (
          <PrMergedIcon />
        )}
        {label}
      </MergeControl>
      {issueId ? (
        <StackMergeConfirmDialog
          confirm={stackConfirm}
          busy={merging}
          onCancel={() => setStackConfirm(null)}
          onConfirm={(input) => void mergeStack(input)}
        />
      ) : null}
      {/* The card is portalled, but React still bubbles its clicks through
          this tree: the span keeps them off the list row the button sits in. */}
      <span className="contents" onClick={(e) => e.stopPropagation()}>
        <Prompt
          open={confirmOpen}
          onOpenChange={setConfirmOpen}
          busy={merging}
          title={mergeCopy.title}
          body={mergeCopy.body}
          actions={promptActions(mergeCopy, {
            merge: { busy: merging, leading: <PrMergedIcon />, onSelect: merge },
          })}
        />
      </span>
    </>
  )
}

/** EXP-895: THE merge control of a Changes surface — the primary glass pill,
 *  the same confirm/spinner/conflict-recovery behaviour. Every Changes
 *  surface renders exactly ONE of these (the review's top bar, the run's, the
 *  phone work bar's capsule); it self-hides unless the PR is open. */
export function SessionMergePill(
  props: Omit<ComponentProps<typeof SessionMergeButton>, `as` | `variant` | `size`>
) {
  return <SessionMergeButton {...props} as="pill" />
}
