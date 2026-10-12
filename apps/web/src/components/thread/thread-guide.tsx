import { useMemo, useState } from "react"
import { PrGithubButton, parseSessionResultGroups, useIsMobile } from "@exp/ui"
import type { CodingSession, Issue } from "@/db/schema"
import { GuideBody, guideFilesOf } from "@/components/guide-face"
import { GuideSectionDiff } from "@/components/guide-section-diff"
import { MergeCapsule } from "@/components/session-merge-button"
import { renderResultText } from "@/components/agent-session"
import { mergeTargetProps } from "@/hooks/use-agents-data"
import { useReviewFiles, useSessionPrFiles } from "@/hooks/use-review-files"
import { guideSectionPage, type GuideSectionKey } from "@/lib/work-faces"
import { threadPrUrl } from "@/lib/thread-page"
import type { ThreadBundle } from "@/hooks/use-threads"

// EXP-1284: the thread's GUIDE child — the existing Guide (`GuideBody`, its
// section pages) fed the THREAD's Guide (`threads.results`, else the newest
// run's own), over the diff of the thread's PR (the PR of its newest run
// with one, else the one its issues share), with that PR's white Merge
// capsule and its GitHub button.

export function ThreadGuide({
  bundle,
  section,
  file,
  onSection,
  canMerge,
  steerEnabled,
}: {
  bundle: ThreadBundle
  section: GuideSectionKey | undefined
  file: string | null
  onSection: (section: GuideSectionKey | undefined) => void
  /** A member may merge (the server re-checks). */
  canMerge: boolean
  steerEnabled: boolean
}) {
  const isMobile = useIsMobile()
  const results = useMemo(() => {
    if (bundle.thread.results != null) return bundle.thread.results
    const newest = [...bundle.runs].reverse().find((run) => run.results != null)
    return newest?.results ?? null
  }, [bundle])
  const groups = useMemo(() => parseSessionResultGroups(results), [results])
  const prUrl = threadPrUrl(bundle.runs, bundle.issues)
  const prIssue: Issue | null =
    (prUrl && bundle.issues.find((issue) => issue.prUrl === prUrl)) || null
  const prRun: CodingSession | null =
    prIssue || !prUrl
      ? null
      : ([...bundle.runs].reverse().find((run) => run.prUrl === prUrl) ?? null)
  const issueFiles = useReviewFiles(prIssue, { enabled: Boolean(prIssue) })
  const runFiles = useSessionPrFiles(prRun, { enabled: Boolean(prRun) })
  const filesState = prIssue ? issueFiles.state : prRun ? runFiles.state : null
  const files = guideFilesOf(filesState)
  const reload = prIssue ? issueFiles.reload : runFiles.reload
  const [selected, setSelected] = useState<string | null>(file)
  const page =
    section !== undefined ? guideSectionPage(groups, files, section) : null

  const merge = prIssue
    ? mergeTargetProps({ kind: `issue`, issue: prIssue })
    : prRun
      ? mergeTargetProps({ kind: `session`, session: prRun })
      : null
  const mergeable = canMerge && merge?.prState === `open`

  return (
    <div className="relative flex min-h-full flex-col" data-testid="thread-guide">
      {prUrl && !page && (
        <div className="flex justify-end px-4 pt-3 md:px-5">
          <PrGithubButton prUrl={prUrl} />
        </div>
      )}
      <div className="flex-1">
        {page ? (
          <GuideSectionDiff
            page={page}
            selected={selected}
            onSelect={setSelected}
            onBack={() => onSection(undefined)}
            isMobile={isMobile}
          />
        ) : (
          <GuideBody
            groups={groups}
            files={files}
            filesState={filesState}
            onOpenChanges={(target) => onSection(target.section)}
            onRetry={() => void reload()}
            renderText={renderResultText}
          />
        )}
      </div>
      {mergeable && merge && (
        <div className="sticky bottom-4 mt-6 flex justify-center pb-4">
          <MergeCapsule {...merge} steerEnabled={steerEnabled} />
        </div>
      )}
    </div>
  )
}
