import { useMemo, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import {
  Button,
  conceptIcon,
  GlassSectionHeader,
  Menu,
  type MenuEntry,
  parseSessionResultGroups,
  StackRail,
  useIsMobile,
} from "@exp/ui"
import { GuideBody, guideFilesOf } from "@/components/guide-face"
import {
  GuideSectionDiff,
  guideSectionPage,
  type GuideSectionKey,
} from "@/components/guide-section-diff"
import { PullRequestMergePill, usePullRequestClose } from "@/components/session-merge-button"
import { StackMergeConfirmDialog } from "@/components/stack-merge-choice-dialog"
import { renderThreadText } from "@/components/thread/thread-markdown"
import { usePullRequestFiles } from "@/hooks/use-review-files"
import {
  usePullRequestStackConfirm,
  usePullRequestStackView,
} from "@/hooks/use-stack-merge-choice"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { pullRequestCollection } from "@/lib/collections"
import { mergeFailure } from "@/lib/merge-failure"
import {
  guideMenuEntries,
  guidePullRequest,
  THREAD_COPY,
  THREAD_TEST_IDS,
  type PullRequestRow,
  type SessionRow,
} from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"

// EXP-1293 Threads v2 / EXP-1306: the Guide tab = the ROOT's ONE Guide
// (`sessions.guide`, every PR body) over ONE pull request, `guidePullRequest`
// (session-thread.json, ×4): its files feed the Guide's Changes rows (a row
// opens that section's diff as a page under the Guide, `?section=`). When
// that PR sits in a linear open stack, the Stack band + THE StackRail lead
// the Guide (a row opens that PR's thread, a hovered row merges through);
// the route closes the tab with THE bottom Merge capsule. No PR rows.
// EXP-1307: Close PR = the header's `…` on the Guide (EXP-1338: no GitHub entry).

const PrStackIcon = conceptIcon(`pr-stack`)
const MoreIcon = conceptIcon(`ui-more`)

/** The base row's label when the bottom PR's base is unknown: never an
 *  assumed `main` (default branches resolve live). */
const STACK_BASE_FALLBACK = `default branch`

/** The Guide's PR (`guidePullRequest` over the root's synced rows). */
export function threadGuidePullRequest(
  pullRequests: readonly PullRequestRow[]
): PullRequestRow | null {
  const byId = new Map(pullRequests.map((pr) => [pr.id, pr]))
  const picked = guidePullRequest(
    pullRequests.map((pr) => ({
      id: pr.id,
      number: pr.number,
      state: pr.state,
      createdAt: new Date(pr.createdAt).toISOString(),
    }))
  )
  return picked ? (byId.get(picked.id) ?? null) : null
}

/** Merge through a stack member: the ONE stack confirm in `through` mode. */
function MergeThroughConfirm({
  pr,
  onDone,
}: {
  pr: PullRequestRow | null
  onDone: () => void
}) {
  const stack = usePullRequestStackConfirm(pr, `through`)
  const [merging, setMerging] = useState(false)
  const merge = async (pullRequestId: string) => {
    setMerging(true)
    try {
      const result = await trpc.pullRequests.merge.mutate(
        { pullRequestId, stack: true },
        SKIP_ERROR_TOAST
      )
      if (result.error) {
        failedAction(`merge`, `the stack`, result.error)
      } else if (result.txid !== undefined) {
        await pullRequestCollection.utils.awaitTxId(result.txid).catch(() => undefined)
      }
    } catch (error) {
      // EXP-1391: every refusal offers Retry.
      failedAction(`merge`, `the stack`, mergeFailure(error, `The pull request could not be merged`).message, {
        retry: () => void merge(pullRequestId),
      })
    } finally {
      setMerging(false)
      onDone()
    }
  }
  return (
    <StackMergeConfirmDialog
      confirm={pr ? stack.confirm : null}
      busy={merging}
      onCancel={onDone}
      onConfirm={(input) => void merge(input.issueId)}
    />
  )
}

function GuideStackCard({
  pr,
  teamSlug,
  canMerge,
}: {
  pr: PullRequestRow
  teamSlug: string
  canMerge: boolean
}) {
  const navigate = useNavigate()
  const view = usePullRequestStackView(pr)
  const [through, setThrough] = useState<PullRequestRow | null>(null)
  if (!view) return null
  const members = view.rows.map((row) => ({
    key: row.issueId,
    identifier: row.identifier,
    title: row.title,
    current: row.isCurrent,
  }))
  return (
    <div className="flex flex-col" data-testid={THREAD_TEST_IDS.guideStack}>
      <GlassSectionHeader
        label={THREAD_COPY.stack}
        leading={<PrStackIcon className="size-3.5 shrink-0 text-muted-foreground" />}
      />
      <StackRail
        members={members}
        baseBranch={view.baseBranch ?? STACK_BASE_FALLBACK}
        onOpen={(member) => {
          if (member.current) return
          const rootId = pullRequestCollection.get(member.key)?.rootSessionId
          if (!rootId) return
          void navigate({
            to: `/t/$teamSlug/threads/$threadId`,
            params: { teamSlug, threadId: rootId },
            search: { tab: `guide` },
          })
        }}
        onMergeThrough={
          canMerge
            ? (member) => setThrough(pullRequestCollection.get(member.key) ?? null)
            : undefined
        }
      />
      <MergeThroughConfirm pr={through} onDone={() => setThrough(null)} />
    </div>
  )
}

/** The thread header's `…` on the Guide (`guideMenuEntries`, ×4): Close PR
 *  while the PR is open and the viewer is a member (EXP-1338: no GitHub
 *  entry, only the agent opens PRs). */
export function ThreadGuideMenu({
  pr,
  canClose,
  className,
}: {
  pr: PullRequestRow
  canClose: boolean
  className?: string
}) {
  const [open, setOpen] = useState(false)
  const close = usePullRequestClose(pr, canClose)
  const entries = guideMenuEntries({ state: pr.state, member: canClose }).flatMap(
    (): MenuEntry[] => (close.entry ? [close.entry] : [])
  )
  if (entries.length === 0) return null
  return (
    <>
      <Menu
        open={open}
        onOpenChange={setOpen}
        align="end"
        aria-label={`#${pr.number}`}
        title={`#${pr.number}`}
        contentClassName="w-64"
        entries={entries}
        trigger={
          <Button
            type="button"
            variant="ghost"
            size="icon"
            aria-label="More"
            data-testid={THREAD_TEST_IDS.guideMenu}
            className={className}
          >
            <MoreIcon />
          </Button>
        }
      />
      {close.dialog}
    </>
  )
}

/** THE white bottom Merge capsule of the Guide's open PR. */
export function ThreadGuideMerge({
  pr,
  canMerge,
  placement = `bottom`,
}: {
  pr: PullRequestRow | null
  canMerge: boolean
  /** EXP-1316: `header` = md+, the thread header's trailing slot on every
   *  tab; `bottom` = phones, THE white capsule closing the Guide tab. */
  placement?: `bottom` | `header`
}) {
  if (!pr || pr.state !== `open` || !canMerge) return null
  if (placement === `header`) {
    return (
      <div className="flex shrink-0 items-center" data-testid={THREAD_TEST_IDS.guideMerge}>
        <PullRequestMergePill pr={pr} canMerge={canMerge} />
      </div>
    )
  }
  return (
    <div
      className="flex shrink-0 justify-center px-4 pt-2 pb-4"
      data-testid={THREAD_TEST_IDS.guideMerge}
    >
      <PullRequestMergePill pr={pr} canMerge={canMerge} />
    </div>
  )
}

export function ThreadGuide({
  root,
  pullRequest,
  teamSlug,
  section,
  file,
  onSection,
  canMerge,
}: {
  root: Pick<SessionRow, `guide`>
  /** `threadGuidePullRequest` of the root's PRs. */
  pullRequest: PullRequestRow | null
  teamSlug: string
  section: GuideSectionKey | undefined
  file: string | null
  onSection: (section: GuideSectionKey | undefined, file?: string | null) => void
  /** A member may merge (the server re-checks). */
  canMerge: boolean
}) {
  const isMobile = useIsMobile()
  const groups = useMemo(() => parseSessionResultGroups(root.guide ?? null), [root.guide])
  const files = usePullRequestFiles(pullRequest?.id ?? null)
  const diff = pullRequest ? guideFilesOf(files.state) : null
  const page = section !== undefined ? guideSectionPage(groups, diff, section) : null

  if (page) {
    return (
      <GuideSectionDiff
        page={page}
        selected={file}
        onSelect={(path) => onSection(section, path)}
        onBack={() => onSection(undefined)}
        isMobile={isMobile}
      />
    )
  }

  return (
    <div className="flex min-h-full flex-col" data-testid="thread-guide">
      <GuideBody
        groups={groups}
        files={diff}
        filesState={pullRequest ? files.state : { kind: `none` }}
        stack={
          pullRequest ? (
            <GuideStackCard pr={pullRequest} teamSlug={teamSlug} canMerge={canMerge} />
          ) : null
        }
        onOpenChanges={(target) => onSection(target.section as GuideSectionKey)}
        onRetry={files.reload}
        renderText={renderThreadText}
      />
    </div>
  )
}
