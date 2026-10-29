import { useMemo, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { additionsLabel, deletionsLabel, type DiffFile } from "@exp/domain-contract/diff"
import { STORY_TAB } from "@exp/domain-contract/stories"
import {
  Button,
  conceptIcon,
  DiffCounts,
  FileDiffList,
  FileDiffTree,
  GlassSectionHeader,
  GuideChangesRow,
  guideFileCountLabel,
  SkeletonRows,
  StackRail,
  StoryTab,
  StoryViewer,
  useIsMobile,
} from "@exp/ui"
import { StackMergeConfirmDialog } from "@/components/stack-merge-choice-dialog"
import { renderThreadText } from "@/components/thread/thread-markdown"
import { usePullRequestFiles } from "@/hooks/use-review-files"
import { usePullRequestStackConfirm, usePullRequestStackView } from "@/hooks/use-stack-merge-choice"
import { useSessionNav } from "@/hooks/use-session-nav"
import type { StoryTabState } from "@/hooks/use-stories"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { pullRequestCollection } from "@/lib/collections"
import { emptyStateCopy } from "@/lib/empty-states"
import { mergeFailure } from "@/lib/merge-failure"
import type { PullRequestRow } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"

// STORIES TAB ×web (stories.json STORIES TAB): the session screen's last
// tab. Top: the PR block while the thread has its ONE pull request
// (`guidePullRequest`): the Stack band + THE StackRail when it sits in a
// linear open stack, then ONE Changes row opening THE review pane (the
// complete diff as a page, `?tab=stories&section=all&file=`, back row
// `‹ Stories`). Then THE tiles (`storyTab()`); a tap opens THE viewer on the
// tab's ring at that story.

const PrStackIcon = conceptIcon(`pr-stack`)
const UiBackIcon = conceptIcon(`ui-back`)

/** The base row's label when the bottom PR's base is unknown: never an
 *  assumed `main` (default branches resolve live). */
const STACK_BASE_FALLBACK = `default branch`

/** The md+ tree column, the IDE's `FILE_LIST_WIDTH`. */
const REVIEW_TREE_WIDTH = 216

/** Merge through a stack member: the ONE stack confirm in `through` mode. */
function MergeThroughConfirm({ pr, onDone }: { pr: PullRequestRow | null; onDone: () => void }) {
  const stack = usePullRequestStackConfirm(pr, `through`)
  const [merging, setMerging] = useState(false)
  const merge = async (pullRequestId: string) => {
    setMerging(true)
    try {
      const result = await trpc.pullRequests.merge.mutate({ pullRequestId, stack: true }, SKIP_ERROR_TOAST)
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

/** The Stack band + THE StackRail (the Guide's): a row opens that PR's
 *  thread on its Stories tab, a hovered row merges through it. */
function StoriesStackCard({ pr, teamSlug, canMerge }: { pr: PullRequestRow; teamSlug: string; canMerge: boolean }) {
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
    <div className="flex flex-col" data-testid={STORY_TAB.testIds.stack}>
      <GlassSectionHeader
        label={STORY_TAB.copy.stack}
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
            search: { tab: `stories` },
          })
        }}
        onMergeThrough={canMerge ? (member) => setThrough(pullRequestCollection.get(member.key) ?? null) : undefined}
      />
      <MergeThroughConfirm pr={through} onDone={() => setThrough(null)} />
    </div>
  )
}

const sum = (files: DiffFile[]) => ({
  files,
  fileCount: files.length,
  additions: files.reduce((total, file) => total + file.additions, 0),
  deletions: files.reduce((total, file) => total + file.deletions, 0),
})

/** THE review pane: the PR's complete diff as a page under the tab. md+ the
 *  file tree beside the file cards, phones the cards alone. */
function StoriesReview({
  files,
  selected,
  onSelect,
  onBack,
}: {
  files: DiffFile[]
  selected: string | null
  onSelect: (path: string) => void
  onBack: () => void
}) {
  const isMobile = useIsMobile()
  const counts = sum(files)
  return (
    <div className="flex flex-col" data-testid="thread-stories-review">
      <div className="flex min-w-0 items-center gap-2 px-4 pt-4 pb-2 md:px-5">
        <Button
          variant="ghost"
          size="sm"
          className="shrink-0 gap-1.5 px-2 font-normal"
          onClick={onBack}
          data-testid="thread-stories-review-back"
        >
          <UiBackIcon className="size-4" />
          {STORY_TAB.copy.label}
        </Button>
        <span aria-hidden className="h-4 w-px shrink-0 bg-glass-stroke" />
        <span className="min-w-0 flex-1 truncate text-sm font-medium">{STORY_TAB.copy.changes}</span>
        <span
          className="flex shrink-0 items-center gap-1 text-xs text-muted-foreground tabular-nums"
          title={`${additionsLabel(counts.additions)} ${deletionsLabel(counts.deletions)} · ${guideFileCountLabel(counts.fileCount)}`}
        >
          <DiffCounts additions={counts.additions} deletions={counts.deletions} className="text-xs" />
          {!isMobile && <span>{`· ${guideFileCountLabel(counts.fileCount)}`}</span>}
        </span>
      </div>
      <div className="flex min-w-0 gap-3 md:px-5">
        {!isMobile && files.length > 1 && (
          <aside className="sticky top-0 hidden shrink-0 self-start md:block" style={{ width: REVIEW_TREE_WIDTH }}>
            <FileDiffTree files={files} selected={selected} onSelect={onSelect} />
          </aside>
        )}
        <FileDiffList
          files={files}
          nav="none"
          focusPath={selected}
          onSelect={onSelect}
          emptyLabel={emptyStateCopy(`changed-files`).title}
          className="min-w-0 flex-1 md:px-0"
        />
      </div>
    </div>
  )
}

export function ThreadStories({
  stories: tab,
  teamSlug,
  pullRequest,
  canMerge,
  canReply,
  review,
  file,
  onReview,
  here,
  back,
}: {
  /** `useStoryTab` of the root (the screen holds it: the chip's dot). */
  stories: StoryTabState
  teamSlug: string
  /** `threadGuidePullRequest` of the root's PRs. */
  pullRequest: PullRequestRow | null
  canMerge: boolean
  /** A member may answer a waiting story. */
  canReply: boolean
  /** The review pane is open (`?section=all`). */
  review: boolean
  file: string | null
  onReview: (open: boolean, file?: string | null) => void
  /** The session on show (the viewer's header pushes the back stack). */
  here: string
  back: string | undefined
}) {
  const isMobile = useIsMobile()
  const files = usePullRequestFiles(pullRequest?.id ?? null)
  const diff = files.state.kind === `files` ? files.state.files : null
  const counts = useMemo(() => (diff ? sum(diff) : null), [diff])
  const nav = useSessionNav({ teamSlug, here, back })
  const [open, setOpen] = useState<{ storyId: string; picture: number } | null>(null)
  const rings = useMemo(() => (tab.ring ? [tab.ring] : []), [tab.ring])

  if (review && diff) {
    return <StoriesReview files={diff} selected={file} onSelect={(path) => onReview(true, path)} onBack={() => onReview(false)} />
  }

  const lead = pullRequest ? (
    <div className="flex flex-col gap-3">
      <StoriesStackCard pr={pullRequest} teamSlug={teamSlug} canMerge={canMerge} />
      {counts && counts.fileCount > 0 && (
        <div data-testid={STORY_TAB.testIds.changes}>
          <GuideChangesRow changes={counts} label={STORY_TAB.copy.changes} onOpen={() => onReview(true)} />
        </div>
      )}
    </div>
  ) : null

  return (
    <>
      {tab.loading && !tab.ring ? (
        <SkeletonRows />
      ) : (
        <StoryTab
          ring={tab.ring}
          size={isMobile ? `phone` : `wide`}
          lead={lead}
          onOpen={(story, picture) => setOpen({ storyId: story.id, picture })}
        />
      )}
      <StoryViewer
        rings={rings}
        openRing={open && tab.ring ? tab.ring.key : null}
        openStory={open?.storyId ?? null}
        openPicture={open?.picture ?? 0}
        onClose={() => setOpen(null)}
        avatarFor={tab.avatarFor}
        cardFor={tab.cardFor}
        onShown={tab.markShown}
        onOpenSession={nav.open}
        onDecide={canReply ? tab.onDecide : undefined}
        onAnswer={canReply ? tab.onAnswer : undefined}
        pending={tab.pending}
        renderText={renderThreadText}
      />
    </>
  )
}
