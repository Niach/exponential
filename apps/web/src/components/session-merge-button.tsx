import { useEffect, useState, type ReactNode } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import closePrCopy from "@exp/domain-contract/fixtures/close-pr.json"
import { contract } from "@exp/domain-contract"
import { mergePill } from "@exp/domain-contract/pr-checks"
import {
  conceptIcon,
  FEEDBACK_WORDS,
  PrMergePill,
  Prompt,
  type MenuEntry,
} from "@exp/ui"
import { closePrBody } from "@/components/close-pr-dialog"
import { StackMergeConfirmDialog } from "@/components/stack-merge-choice-dialog"
import { usePullRequestStackConfirm } from "@/hooks/use-stack-merge-choice"
import { failingChecksNames, usePullRequestChecks } from "@/hooks/use-pull-request-checks"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { pullRequestCollection, pullRequestIssueCollection } from "@/lib/collections"
import { mergeFailure } from "@/lib/merge-failure"
import { MERGE_STACK_LABEL } from "@/lib/pr-stack"
import {
  mergeFailingPrPrompt,
  mergeIssuePrPrompt,
  mergeRunPrPrompt,
  promptActions,
} from "@/lib/prompts"
import type { PullRequestRow } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"

// EXP-1293 Threads v2: THE merge control = ONE pill over a `pull_requests`
// row (`pullRequests.merge({pullRequestId, stack})`). A PR in an open linear
// stack reads `Merge stack` and asks the ONE stack confirm (it lands the
// chain through its top, EXP-1248); any other open PR asks the plain merge
// confirm. Merging never opens a session: a refusal is a toast and (EXP-1391)
// a line into the thread's root, which takes it over; every refusal offers
// Retry. The spinner holds until the
// echo moves the row off `open`. Close (`pullRequests.close`) rides the
// caller's menu (`usePullRequestClose`), copy = `close-pr.json` ×4.
// #EXP-1611 (pr-checks.json MERGE PILL): the pill wears the PR's
// `checks_state` (`mergePill`): pending = THE Spinner in place of the glyph,
// failure = the plain pill `Checks failing` whose tap asks prompts.json
// `merge-failing-pr` INSTEAD OF the plain/stack confirm; its Merge merges
// exactly as the pill would have (a stack member through its top). The
// person's merge never sends `force`.

export const MERGE_PR_LABEL = contract.diffUi.mergePr
export { MERGE_STACK_LABEL }

const PrMergedIcon = conceptIcon(`pr-merged`)
const PrClosedIcon = conceptIcon(`pr-closed`)

type MergeablePr = Pick<
  PullRequestRow,
  `id` | `number` | `head` | `base` | `state` | `url` | `repositoryId`
> & {
  /** `pull_requests.checks_state` (contract prChecksState). Typed loosely:
   *  drizzle-zod infers the contract-array enum as a non-string union. */
  checksState?: unknown
}

/** How many issues a PR closes (the plain confirm's body). */
function useLinkedIssueCount(pullRequestId: string, enabled: boolean): number {
  const { data } = useLiveQuery(
    (query) =>
      enabled
        ? query
            .from({ links: pullRequestIssueCollection })
            .where(({ links }) => eq(links.pullRequestId, pullRequestId))
        : undefined,
    [enabled, pullRequestId]
  )
  return (data ?? []).length
}

export function PullRequestMergePill({
  pr,
  canMerge = true,
  className,
}: {
  pr: MergeablePr
  /** A member may merge (the server re-checks); false hides it. */
  canMerge?: boolean
  className?: string
}) {
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [stackOpen, setStackOpen] = useState(false)
  const [failingOpen, setFailingOpen] = useState(false)
  const [merging, setMerging] = useState(false)
  const open = pr.state === `open`
  const stack = usePullRequestStackConfirm(open && canMerge ? pr : null)
  const linked = useLinkedIssueCount(pr.id, open && canMerge)
  const view = mergePill({ state: typeof pr.checksState === `string` ? pr.checksState : null, stack: Boolean(stack.confirm) })
  const failing = view.confirm === `failing`
  const checks = usePullRequestChecks(open && canMerge && failing ? pr.id : null)
  const failingCopy = mergeFailingPrPrompt({
    number: pr.number,
    stack: Boolean(stack.confirm),
    names: failingChecksNames(checks),
  })
  const copy =
    linked > 0
      ? mergeIssuePrPrompt({ number: pr.number, count: linked })
      : mergeRunPrPrompt(pr.number)

  useEffect(() => {
    if (open) return
    setMerging(false)
    setConfirmOpen(false)
    setStackOpen(false)
    setFailingOpen(false)
  }, [open])

  if (!open || !canMerge) return null

  const run = async (input: { pullRequestId: string; stack?: boolean }) => {
    setMerging(true)
    const what = input.stack ? `the stack` : `the PR`
    try {
      const result = await trpc.pullRequests.merge.mutate(input, SKIP_ERROR_TOAST)
      setConfirmOpen(false)
      setStackOpen(false)
      setFailingOpen(false)
      if (result.error) {
        setMerging(false)
        failedAction(`merge`, what, result.error)
        return
      }
      // `merging` holds until the echo flips the row off `open`; a merge
      // that wrote nothing (queued behind a merge queue) carries no txid,
      // so nothing will echo and the pill frees at once.
      if (result.txid === undefined) {
        setMerging(false)
        return
      }
      await pullRequestCollection.utils.awaitTxId(result.txid).catch(() => undefined)
    } catch (error) {
      const failure = mergeFailure(error, `The pull request could not be merged`)
      setMerging(false)
      setConfirmOpen(false)
      setStackOpen(false)
      setFailingOpen(false)
      failedAction(`merge`, what, failure.message, {
        retry: () => void run(input),
      })
    }
  }

  return (
    <>
      <PrMergePill
        view={view}
        merging={merging}
        disabled={merging || !stack.ready}
        aria-label={merging ? FEEDBACK_WORDS.pending.merge : view.label}
        title={view.label}
        data-testid="pr-merge-pill"
        className={className}
        onClick={(event) => {
          event.stopPropagation()
          if (failing) setFailingOpen(true)
          else if (stack.confirm) setStackOpen(true)
          else setConfirmOpen(true)
        }}
      />
      <StackMergeConfirmDialog
        confirm={stackOpen ? stack.confirm : null}
        busy={merging}
        onCancel={() => setStackOpen(false)}
        onConfirm={(input) => void run({ pullRequestId: input.issueId, stack: true })}
      />
      <span className="contents" onClick={(event) => event.stopPropagation()}>
        <Prompt
          open={confirmOpen}
          onOpenChange={setConfirmOpen}
          busy={merging}
          title={copy.title}
          body={copy.body}
          actions={promptActions(copy, {
            merge: {
              busy: merging,
              leading: <PrMergedIcon />,
              onSelect: () => void run({ pullRequestId: pr.id }),
            },
          })}
        />
        <Prompt
          open={failingOpen}
          onOpenChange={setFailingOpen}
          busy={merging}
          title={failingCopy.title}
          body={failingCopy.body}
          actions={promptActions(failingCopy, {
            merge: {
              busy: merging,
              testId: `pr-merge-failing-confirm`,
              onSelect: () =>
                void run(
                  stack.confirm
                    ? { pullRequestId: stack.confirm.input.issueId, stack: true }
                    : { pullRequestId: pr.id }
                ),
            },
          })}
        />
      </span>
    </>
  )
}

/** Close a PR without merging: the menu entry (open PRs, members only) and
 *  its confirm. Render `dialog` once. */
export function usePullRequestClose(
  pr: Pick<PullRequestRow, `id` | `state`>,
  canClose: boolean
): { entry: MenuEntry | null; dialog: ReactNode } {
  const [open, setOpen] = useState(false)
  const [closing, setClosing] = useState(false)
  const allowed = canClose && pr.state === `open`
  const linked = useLinkedIssueCount(pr.id, allowed)
  const close = async () => {
    setClosing(true)
    try {
      const result = await trpc.pullRequests.close.mutate({ pullRequestId: pr.id }, SKIP_ERROR_TOAST)
      setOpen(false)
      if (result.txid !== undefined) {
        await pullRequestCollection.utils.awaitTxId(result.txid).catch(() => undefined)
      }
    } catch (error) {
      failedAction(`close`, `the PR`, mergeFailure(error, `The pull request could not be closed`).message)
    } finally {
      setClosing(false)
    }
  }
  return {
    entry: allowed
      ? {
          kind: `item`,
          id: `close-pr`,
          label: closePrCopy.menuItem,
          icon: PrClosedIcon,
          destructive: true,
          onSelect: () => setOpen(true),
          "data-testid": `pr-close`,
        }
      : null,
    dialog: (
      <Prompt
        open={open}
        onOpenChange={setOpen}
        busy={closing}
        title={closePrCopy.title}
        body={closePrBody(Math.max(0, linked - 1))}
        actions={[
          { label: `Cancel`, role: `cancel` },
          {
            label: closePrCopy.confirm,
            role: `destructive`,
            busy: closing,
            onSelect: () => void close(),
          },
        ]}
      />
    ),
  }
}
