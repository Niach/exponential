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
import type { StackMergeChoice } from "@/lib/pr-stack"
import { trpc } from "@/lib/trpc-client"
import { useOpenComposer } from "@/hooks/use-open-composer"
import { useStackMergeChoice } from "@/hooks/use-stack-merge-choice"
import { useLinkedIssueCount } from "@/hooks/use-linked-issue-count"
import {
  StackMergeChoiceDialog,
  type StackMergeInput,
} from "@/components/stack-merge-choice-dialog"
import type { VariantProps } from "class-variance-authority"

/** EXP-917: the ONE gate on the "Fix conflicts" swap — a REAL conflict
 *  (EXP-533), on an ISSUE target (the builtin action takes a representative
 *  issue, so a run's own chore PR never qualifies), with a recorded branch
 *  (the run rebases it) and the relay configured. Pure, so the rule is a
 *  test; mirrors Android `canOfferFixConflicts`, iOS `canFixConflicts` and
 *  desktop `work_header::merge_slot_swapped`. */
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
const UiBranchIcon = conceptIcon(`ui-branch`)

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
// settles on the session row's own echo. Recovery ("Fix conflicts") stays
// issue-only: the builtin action takes a representative ISSUE id, so a run
// PR's conflict just reports its refusal.
//
// EXP-706: when the merge is refused by a REAL conflict (EXP-533) and the
// caller wired the recovery run (`branch` + `steerEnabled`), this button
// REPLACES itself with "Fix conflicts" in the very same slot — the same swap
// the Reviews list and the review detail make. Every other refusal still
// reaches the user as a toast; nothing is swallowed.
//
// EXP-917: the swap takes NOTHING from the caller that a synced issue row
// cannot supply. It used to require a `teamId` too, and every surface fed it
// `issue.teamId` — a column the board-scoped `issues` shape deliberately drops
// (REV2-5), so the gate was dead on the property tray, the run header, the
// Changes faces and the review detail: a real conflict fell through to the
// toast. The team is not needed: the composer resolves it from the route.
//
// A refusal describes ONE snapshot of the pull request, so the swap is
// deliberately short-lived: a newer `updatedAt` (the Electric echo of a
// re-synced row) drops it, and while it stands a secondary "Retry merge"
// button keeps the plain merge one click away. Without both, a conflict
// resolved OUTSIDE the recovery run (a teammate rebases and pushes, GitHub
// recomputes mergeability) would hide Merge for the life of the open PR.
//
// EXP-1145: a PR that is a member of a STACK of 2+ open pull requests never
// merges off the plain confirm. The click first reads the stack off the
// synced rows (`useStackMergeChoice`, armed by the click so a list of these
// buttons costs no live queries) and opens `StackMergeChoiceDialog`: Merge
// stack, Merge this pull request (plain for the bottom member, the chain
// through this one otherwise), Cancel. A run's own chore PR is in no stack.

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
  updatedAt,
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
  /** Member + relay configured (`useSteerConfig`), resolved by the caller. */
  steerEnabled?: boolean
}) {
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [merging, setMerging] = useState(false)
  const [failure, setFailure] = useState<MergeFailure | null>(null)
  // EXP-1145: the click ARMS the stack read; the answer decides which dialog
  // opens. `stackChoice` holds the stack dialog's content while it is open.
  const [armed, setArmed] = useState(false)
  const [stackChoice, setStackChoice] = useState<StackMergeChoice | null>(null)
  const stack = useStackMergeChoice(issueId, armed)
  const linkedCount = useLinkedIssueCount(issueId, confirmOpen)
  const mergeCopy = issueId
    ? mergeIssuePrPrompt({ number: prNumber, count: linkedCount })
    : mergeRunPrPrompt(prNumber)
  const stamp =
    updatedAt instanceof Date ? updatedAt.toISOString() : (updatedAt ?? null)

  useEffect(() => {
    if (prState !== `open`) {
      setMerging(false)
      setConfirmOpen(false)
      setArmed(false)
      setStackChoice(null)
      setFailure(null)
    }
  }, [prState])

  // The armed click resolves as soon as the rows are in: a stack member opens
  // the stack dialog, anything else the plain confirm.
  useEffect(() => {
    if (!armed || !stack.ready) return
    setArmed(false)
    if (stack.choice) setStackChoice(stack.choice)
    else setConfirmOpen(true)
  }, [armed, stack.ready, stack.choice])

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
  const canFixConflicts = canOfferFixConflicts({
    failure,
    issueId,
    branch,
    steerEnabled,
  })

  // The click itself: a session PR goes straight to the plain confirm, an
  // issue PR first asks the synced rows about its stack.
  const arm = () => {
    if (issueId) setArmed(true)
    else setConfirmOpen(true)
  }

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
      setStackChoice(null)
    } catch (error) {
      const next = mergeFailure(
        error,
        `The pull request could not be merged`
      )
      setMerging(false)
      setConfirmOpen(false)
      setStackChoice(null)
      setFailure(next)
      // The swap is this button's own caption for a conflict; every other
      // refusal has nowhere to live in a row this small, so it keeps the
      // global toast the link would otherwise have shown.
      if (
        !canOfferFixConflicts({ failure: next, issueId, branch, steerEnabled })
      ) {
        toast.error(`Couldn't merge the pull request`, {
          description: next.message,
        })
      }
    }
  }

  // EXP-1145: a stack merge (Merge stack, or Merge this pull request on a
  // member above the bottom) lands several pull requests; its refusal is the
  // server's message about whichever one stopped the chain, never a
  // rebase-and-resolve job for THIS one, so it is toasted, not swapped.
  const mergeStack = async (input: StackMergeInput) => {
    setMerging(true)
    setFailure(null)
    try {
      await trpc.issues.mergePr.mutate(input, {
        context: { skipErrorToast: true },
      })
      setStackChoice(null) // keep `merging` until the echo flips prState
    } catch (error) {
      const next = mergeFailure(error, `The stack could not be merged`)
      setMerging(false)
      setStackChoice(null)
      toast.error(`Couldn't merge the stack`, { description: next.message })
    }
  }

  const showFix = canFixConflicts && issueId

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
              away as a quiet secondary. `pointer-events-auto`: the phone's
              bar and Merge float are `pointer-events-none` shells. */}
          <Button
            variant="glass"
            size="icon-sm"
            className="pointer-events-auto"
            disabled={merging}
            aria-label={merging ? `Merging…` : `Retry merge`}
            title={merging ? `Merging…` : `Retry merge`}
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
      )}
      {issueId ? (
        <StackMergeChoiceDialog
          choice={stackChoice}
          issueId={issueId}
          busy={merging}
          onCancel={() => setStackChoice(null)}
          onMerge={(input) => {
            // The bottom member's "Merge this pull request" is the plain
            // single-PR merge, Fix-conflicts swap and all.
            if (input.mergeStack) void mergeStack(input)
            else void merge()
          }}
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
