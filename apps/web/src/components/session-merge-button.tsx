import { useEffect, useState, type ReactNode } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import closePrCopy from "@exp/domain-contract/fixtures/close-pr.json"
import { contract } from "@exp/domain-contract"
import { MERGE_TRAIN_COPY, MERGE_TRAIN_TEST_IDS, statusPill, type HoldReason, type MergeWait } from "@exp/domain-contract/merge-train"
import { PR_CHECKS_COPY } from "@exp/domain-contract/pr-checks"
import {
  conceptIcon,
  FEEDBACK_WORDS,
  Menu,
  prChecksMenuEntry,
  PrStatusPill,
  Prompt,
  type MenuEntry,
} from "@exp/ui"
import { closePrBody } from "@/components/close-pr-dialog"
import { StackMergeConfirmDialog } from "@/components/stack-merge-choice-dialog"
import { usePullRequestStackConfirm } from "@/hooks/use-stack-merge-choice"
import { usePullRequestChecks } from "@/hooks/use-pull-request-checks"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { pullRequestCollection, pullRequestIssueCollection } from "@/lib/collections"
import { mergeFailure } from "@/lib/merge-failure"
import { MERGE_STACK_LABEL } from "@/lib/pr-stack"
import { mergeIssuePrPrompt, mergeNowPrPrompt, mergeRunPrPrompt, promptActions } from "@/lib/prompts"
import { prMergeState, type PullRequestRow } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"

// #EXP-1710 (merge-train.json STATUS PILL, replaces EXP-1293's Merge pill):
// THE header pill is the PR's merge STATUS (`statusPill` over the synced
// `pull_requests.merge_*` columns the train worker writes). Its tap opens the
// Checks list (pr-checks.json CHECKS LIST; no rows = readonly). THE ONLY
// action: `approval` mode, a ready PR nobody approved = the primary `Needs
// approval`, whose tap asks the plain or stack confirm (prompts.json
// merge-issue-pr / merge-run-pr / the stack confirm) and whose Merge QUEUES
// it (`pullRequests.merge`, the train on: never a direct merge). Close
// (`pullRequests.close`) rides the caller's menu (`usePullRequestClose`),
// copy = `close-pr.json` ×4.

export const MERGE_PR_LABEL = contract.diffUi.mergePr
export { MERGE_STACK_LABEL }

const PrMergedIcon = conceptIcon(`pr-merged`)
const PrClosedIcon = conceptIcon(`pr-closed`)

type StatusPr = Pick<
  PullRequestRow,
  | `id`
  | `number`
  | `head`
  | `base`
  | `state`
  | `url`
  | `repositoryId`
  | `mergeState`
  | `mergeWait`
  | `mergeHold`
  | `mergePosition`
  | `mergeTrainId`
>

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

/** The PRs a train run tests together (`Testing with N`). */
function useTrainSize(trainId: string | null): number | null {
  const { data } = useLiveQuery(
    (query) =>
      trainId
        ? query.from({ prs: pullRequestCollection }).where(({ prs }) => eq(prs.mergeTrainId, trainId))
        : undefined,
    [trainId]
  )
  return trainId ? Math.max(1, (data ?? []).length) : null
}


/** #EXP-1710: THE Needs approval confirm over one PR (the pill's tap and a
 *  Needs you `approval` row's press share it): `request()` asks the plain
 *  confirm (prompts.json merge-issue-pr / merge-run-pr) or the stack confirm
 *  when the PR has an open stack, and Merge calls `pullRequests.merge`, which
 *  QUEUES it (the train on: never a direct merge). Cancel closes it. Render
 *  `dialog` once; `onSettled` runs when the confirm closes either way. */
export function usePullRequestApproval(
  pr: Pick<PullRequestRow, `id` | `number` | `head` | `base` | `state` | `url` | `repositoryId`> | null,
  onSettled?: () => void
): { ready: boolean; busy: boolean; stack: boolean; request: () => void; dialog: ReactNode } {
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [stackOpen, setStackOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const active = pr !== null && pr.state === `open`
  const stack = usePullRequestStackConfirm(active ? pr : null)
  const linked = useLinkedIssueCount(pr?.id ?? ``, active)
  const copy =
    linked > 0
      ? mergeIssuePrPrompt({ number: pr?.number ?? 0, count: linked })
      : mergeRunPrPrompt(pr?.number ?? 0)

  useEffect(() => {
    if (active) return
    setBusy(false)
    setConfirmOpen(false)
    setStackOpen(false)
  }, [active])

  const settle = () => {
    setConfirmOpen(false)
    setStackOpen(false)
    onSettled?.()
  }

  const approve = async (input: { pullRequestId: string; stack?: boolean }) => {
    setBusy(true)
    const what = input.stack ? `the stack` : `the PR`
    try {
      const result = await trpc.pullRequests.merge.mutate(input, SKIP_ERROR_TOAST)
      settle()
      if (result.error) failedAction(`merge`, what, result.error)
    } catch (error) {
      settle()
      failedAction(`merge`, what, mergeFailure(error, `The pull request could not be merged`).message, {
        retry: () => void approve(input),
      })
    } finally {
      // The echo moves the row off `approval` (queued); until then the
      // spinner shows only while the call is in flight.
      setBusy(false)
    }
  }

  const request = () => {
    if (!active || !stack.ready) return
    if (stack.confirm) setStackOpen(true)
    else setConfirmOpen(true)
  }

  const dialog = active ? (
    <>
      <StackMergeConfirmDialog
        confirm={stackOpen ? stack.confirm : null}
        busy={busy}
        onCancel={settle}
        onConfirm={(input) => void approve({ pullRequestId: input.issueId, stack: true })}
      />
      <span className="contents" onClick={(event) => event.stopPropagation()}>
        <Prompt
          open={confirmOpen}
          onOpenChange={(next) => {
            if (next) setConfirmOpen(true)
            else settle()
          }}
          busy={busy}
          title={copy.title}
          body={copy.body}
          actions={promptActions(copy, {
            merge: {
              busy,
              leading: <PrMergedIcon />,
              onSelect: () => {
                if (pr) void approve({ pullRequestId: pr.id })
              },
            },
          })}
        />
      </span>
    </>
  ) : null

  return { ready: stack.ready, busy, stack: Boolean(stack.confirm), request, dialog }
}

export function PullRequestStatusPill({
  pr,
  member = true,
  className,
}: {
  pr: StatusPr
  /** A member may approve (the server re-checks); a viewer reads only. */
  member?: boolean
  className?: string
}) {
  const [checksOpen, setChecksOpen] = useState(false)
  const open = pr.state === `open`
  const state = prMergeState(pr)
  const approval = open && member && state === `checks` && pr.mergeWait === `approval`
  const approve = usePullRequestApproval(approval ? pr : null)
  const checks = usePullRequestChecks(pr.id)
  const batchSize = useTrainSize(state === `testing` ? (pr.mergeTrainId ?? null) : null)
  const computed = statusPill({
    state,
    wait: (pr.mergeWait ?? null) as MergeWait | null,
    position: pr.mergePosition ?? null,
    batchSize,
    holdReason: (pr.mergeHold ?? null) as HoldReason | null,
    stack: approve.stack,
    checks: checks.length,
  })
  // A viewer's `Needs approval` is a status they cannot act on.
  const view =
    computed.tap === `approve` && !approval
      ? { ...computed, primary: false, tap: checks.length > 0 ? (`checks` as const) : null }
      : computed

  if (pr.state === `closed`) return null

  const pill = (
    <PrStatusPill
      view={view}
      busy={approve.busy}
      disabled={view.tap === `approve` && (approve.busy || !approve.ready)}
      aria-label={approve.busy ? FEEDBACK_WORDS.pending.merge : view.label}
      title={view.label}
      data-testid={MERGE_TRAIN_TEST_IDS.statusPill}
      data-state={state}
      className={className}
      onClick={(event) => {
        event.stopPropagation()
        if (view.tap === `approve`) approve.request()
      }}
    />
  )

  if (view.tap === `checks`) {
    const entry = prChecksMenuEntry(checks, new Date())
    return (
      <span className="contents" onClick={(event) => event.stopPropagation()}>
        <Menu
          open={checksOpen}
          onOpenChange={setChecksOpen}
          align="end"
          title={PR_CHECKS_COPY.checks}
          aria-label={PR_CHECKS_COPY.checks}
          contentClassName="w-64"
          entries={entry?.entries ?? []}
          trigger={pill}
        />
      </span>
    )
  }
  if (view.tap !== `approve`) return pill

  return (
    <>
      {pill}
      {approve.dialog}
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

const PrHoldIcon = conceptIcon(`pr-hold`)
const PrResumeIcon = conceptIcon(`pr-resume`)

/** #EXP-1710 (merge-train.json `menu`): the `…` menu's train rows over one
 *  PR. Merge now (owners) asks THE shared alert card `merge-now-pr` (Merge
 *  destructive, Cancel focused; a stack member's title names the stack) and
 *  calls `pullRequests.mergeNow`; Hold | Resume auto-merge call
 *  `pullRequests.hold`. Render `dialog` once. */
export function usePullRequestTrainRows(
  pr: Pick<PullRequestRow, `id` | `number` | `head` | `base` | `state` | `url` | `repositoryId`> | null
): { mergeNow: MenuEntry; hold: MenuEntry; resume: MenuEntry; dialog: ReactNode } {
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const live = pr !== null && (pr.state === `open` || pr.state === `draft`)
  const stack = usePullRequestStackConfirm(live ? pr : null)
  const copy = mergeNowPrPrompt({ number: pr?.number ?? null, stack: Boolean(stack.confirm) })
  const mergeNow = async () => {
    if (!pr) return
    setBusy(true)
    try {
      await trpc.pullRequests.mergeNow.mutate({ pullRequestId: pr.id }, SKIP_ERROR_TOAST)
      setConfirmOpen(false)
    } catch (error) {
      setConfirmOpen(false)
      failedAction(`merge`, `the PR`, mergeFailure(error, `The pull request could not be merged`).message, {
        retry: () => void mergeNow(),
      })
    } finally {
      setBusy(false)
    }
  }
  const hold = async (next: boolean) => {
    if (!pr) return
    try {
      await trpc.pullRequests.hold.mutate({ pullRequestId: pr.id, hold: next }, SKIP_ERROR_TOAST)
    } catch (error) {
      failedAction(`update`, `auto-merge`, error, { retry: () => void hold(next) })
    }
  }
  return {
    mergeNow: {
      kind: `item`,
      id: `merge-now`,
      label: MERGE_TRAIN_COPY.menu.mergeNow,
      icon: PrMergedIcon,
      onSelect: () => setConfirmOpen(true),
      "data-testid": MERGE_TRAIN_TEST_IDS.mergeNow,
    },
    hold: {
      kind: `item`,
      id: `hold`,
      label: MERGE_TRAIN_COPY.menu.hold,
      icon: PrHoldIcon,
      onSelect: () => void hold(true),
      "data-testid": MERGE_TRAIN_TEST_IDS.hold,
    },
    resume: {
      kind: `item`,
      id: `resume`,
      label: MERGE_TRAIN_COPY.menu.resume,
      icon: PrResumeIcon,
      onSelect: () => void hold(false),
      "data-testid": MERGE_TRAIN_TEST_IDS.resume,
    },
    dialog: (
      <span className="contents" onClick={(event) => event.stopPropagation()}>
        <Prompt
          open={confirmOpen}
          onOpenChange={setConfirmOpen}
          busy={busy}
          title={copy.title}
          body={copy.body}
          actions={promptActions(copy, {
            merge: { busy, onSelect: () => void mergeNow() },
          })}
        />
      </span>
    ),
  }
}
