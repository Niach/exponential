import { useMemo, useState } from "react"
import {
  Button,
  conceptIcon,
  GlassSectionHeader,
  Menu,
  PrGithubButton,
  PrRow,
  parseSessionResultGroups,
  useIsMobile,
} from "@exp/ui"
import { GuideBody, guideFilesOf } from "@/components/guide-face"
import {
  GuideSectionDiff,
  guideSectionPage,
  type GuideSectionKey,
} from "@/components/guide-section-diff"
import { PullRequestMergePill, usePullRequestClose } from "@/components/session-merge-button"
import { renderThreadText } from "@/components/thread/thread-markdown"
import { usePullRequestFiles } from "@/hooks/use-review-files"
import type { PullRequestRow, SessionRow } from "@/lib/session-tree"

// EXP-1293 Threads v2: the Guide tab = the ROOT's ONE Guide (`sessions.guide`,
// every PR body) over its pull requests (`pull_requests.root_session_id`).
// The PR band lists every PR the tree opened (open first), each with its
// GitHub link, THE merge pill and Close in its menu; the picked PR's files
// (`pullRequests.files`) feed the Guide's Changes rows, and a Changes row
// opens that section's diff as a page under the Guide (`?section=`).

const MoreIcon = conceptIcon(`ui-more`)
const PrOpenIcon = conceptIcon(`pr-open`)

/** The PR band's label (no count, list bands carry none). */
export const THREAD_GUIDE_PRS_LABEL = `Pull requests`

const STATE_WORD: Record<string, string> = {
  open: ``,
  draft: `draft`,
  merged: `merged`,
  closed: `closed`,
}

/** The PR the Guide counts by default: the newest open one, else the newest. */
export function primaryPullRequest(prs: readonly PullRequestRow[]): PullRequestRow | null {
  const newest = [...prs].sort((a, b) => b.number - a.number)
  return newest.find((pr) => pr.state === `open`) ?? newest[0] ?? null
}

function GuidePrRow({
  pr,
  active,
  canMerge,
  onPick,
}: {
  pr: PullRequestRow
  active: boolean
  canMerge: boolean
  onPick: () => void
}) {
  const close = usePullRequestClose(pr, canMerge)
  const [menuOpen, setMenuOpen] = useState(false)
  return (
    <>
      <PrRow
        node={pr.state === `open` ? `open` : `base`}
        identifier={`#${pr.number}`}
        title={pr.title ?? pr.head}
        word={STATE_WORD[pr.state] || null}
        active={active}
        onClick={onPick}
        data-testid="thread-guide-pr"
        trailing={
          <div className="flex shrink-0 items-center gap-1.5">
            <PrGithubButton prUrl={pr.url} />
            <PullRequestMergePill pr={pr} canMerge={canMerge} />
            {close.entry && (
              <Menu
                open={menuOpen}
                onOpenChange={setMenuOpen}
                align="end"
                aria-label={`#${pr.number}`}
                title={`#${pr.number}`}
                contentClassName="w-64"
                entries={[close.entry]}
                trigger={
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-xs"
                    aria-label="More"
                    className="text-muted-foreground"
                  >
                    <MoreIcon />
                  </Button>
                }
              />
            )}
          </div>
        }
      />
      {close.dialog}
    </>
  )
}

export function ThreadGuide({
  root,
  pullRequests,
  section,
  file,
  onSection,
  canMerge,
}: {
  root: Pick<SessionRow, `guide`>
  pullRequests: readonly PullRequestRow[]
  section: GuideSectionKey | undefined
  file: string | null
  onSection: (section: GuideSectionKey | undefined, file?: string | null) => void
  /** A member may merge/close (the server re-checks). */
  canMerge: boolean
}) {
  const isMobile = useIsMobile()
  const groups = useMemo(() => parseSessionResultGroups(root.guide ?? null), [root.guide])
  const ordered = useMemo(
    () =>
      [...pullRequests].sort(
        (a, b) =>
          Number(b.state === `open`) - Number(a.state === `open`) || b.number - a.number
      ),
    [pullRequests]
  )
  const [pickedId, setPickedId] = useState<string | null>(null)
  const picked =
    ordered.find((pr) => pr.id === pickedId) ?? primaryPullRequest(pullRequests)
  const files = usePullRequestFiles(picked?.id ?? null)
  const diff = picked ? guideFilesOf(files.state) : null
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

  const prBand =
    ordered.length > 0 ? (
      <div className="flex flex-col" data-testid="thread-guide-prs">
        <GlassSectionHeader
          label={THREAD_GUIDE_PRS_LABEL}
          leading={<PrOpenIcon className="size-3.5 shrink-0 text-muted-foreground" />}
        />
        {ordered.map((pr) => (
          <GuidePrRow
            key={pr.id}
            pr={pr}
            active={ordered.length > 1 && pr.id === picked?.id}
            canMerge={canMerge}
            onPick={() => setPickedId(pr.id)}
          />
        ))}
      </div>
    ) : null

  return (
    <div className="flex min-h-full flex-col" data-testid="thread-guide">
      <GuideBody
        groups={groups}
        files={diff}
        filesState={picked ? files.state : { kind: `none` }}
        stack={prBand}
        onOpenChanges={(target) => onSection(target.section as GuideSectionKey)}
        onRetry={files.reload}
        renderText={renderThreadText}
      />
    </div>
  )
}
