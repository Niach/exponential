import { additionsLabel, deletionsLabel, type DiffFile } from "@exp/domain-contract/diff"
import { contract } from "@exp/domain-contract"
import {
  Button,
  conceptIcon,
  DiffCounts,
  FileDiffList,
  FileDiffTree,
  GUIDE_CHANGES_TOPIC,
  guideCoverage,
  guideFileCountLabel,
  guideSectionCaption,
  type GuideCoverageFile,
} from "@exp/ui"
import { emptyStateCopy } from "@/lib/empty-states"
import { cn } from "@/lib/utils"

// EXP-1251: a Guide section's diff as a PAGE under the Guide
// (`?tab=guide&section=N&file=`; `section=all` = Show complete diff). The
// back row returns to the Guide and names the section (`02 / 06 · title`)
// with the counts it covers. md+: the file tree IN the column (216px, the
// desktop `diff_pane` width) beside the same cards every diff draws, only
// this section's files. Phones: the cards alone; the file sheet rides the
// Guide face's bar, filtered to the same files.

const UiBackIcon = conceptIcon(`ui-back`)

/** The Guide's name ×4. */
const GUIDE_FACE_LABEL = contract.diffUi.guideFace

/** A section of the Guide: its number, the lead, `Other changes` or `all`
 *  (Show complete diff). */
export type GuideSectionKey = number | `lead` | `other` | `all`

/** A `?section=` value, else undefined (a garbage value = the Guide). */
export function parseGuideSection(raw: unknown): GuideSectionKey | undefined {
  if (raw === `lead` || raw === `other` || raw === `all`) return raw
  const value =
    typeof raw === `number` ? raw : typeof raw === `string` && /^\d+$/.test(raw) ? Number(raw) : NaN
  return Number.isInteger(value) && value >= 1 ? value : undefined
}

/** A section page: the back row's caption (`02 / 06`, numbered sections
 *  only), its title, the files it covers and their summed counts. */
export interface GuideSectionPage<F extends GuideCoverageFile> {
  section: GuideSectionKey
  caption: string | null
  title: string
  files: F[]
  additions: number
  deletions: number
}

/** The page `section` opens over `files` (the diff the Guide counts): the
 *  same coverage the Guide's rows read. Null = no such section (a stale
 *  link) or no diff loaded. */
export function guideSectionPage<
  G extends { topic: string; files?: readonly string[] | null },
  F extends GuideCoverageFile,
>(
  groups: readonly G[],
  files: readonly F[] | null | undefined,
  section: GuideSectionKey
): GuideSectionPage<F> | null {
  if (!files) return null
  const coverage = guideCoverage(groups, files)
  const page = (
    title: string,
    caption: string | null,
    set: { files: F[]; additions: number; deletions: number } | null
  ): GuideSectionPage<F> | null =>
    set
      ? { section, caption, title, files: set.files, additions: set.additions, deletions: set.deletions }
      : null
  if (section === `all`) return page(GUIDE_CHANGES_TOPIC, null, coverage.complete)
  if (section === `other`) {
    return coverage.other ? page(coverage.other.topic, null, coverage.other.changes) : null
  }
  if (section === `lead`) {
    return coverage.lead ? page(coverage.lead.group.topic, null, coverage.lead.changes) : null
  }
  const hit = coverage.sections.find((entry) => entry.index === section)
  return hit
    ? page(hit.group.topic, guideSectionCaption(hit.index, hit.total), hit.changes)
    : null
}

/** The md+ tree column, the IDE's `FILE_LIST_WIDTH`. */
export const GUIDE_SECTION_TREE_WIDTH = 216

/** The back row's counts: `+1 204 −331 · 14 files`. */
export function guideSectionSummary(page: Pick<GuideSectionPage<DiffFile>, `files` | `additions` | `deletions`>): string {
  return `${additionsLabel(page.additions)} ${deletionsLabel(page.deletions)} · ${guideFileCountLabel(page.files.length)}`
}

export function GuideSectionDiff({
  page,
  selected,
  onSelect,
  onBack,
  isMobile,
}: {
  page: GuideSectionPage<DiffFile>
  /** The file in focus (`?file=`); null = the top. */
  selected: string | null
  onSelect: (path: string) => void
  onBack: () => void
  isMobile: boolean
}) {
  return (
    <div className="flex flex-col" data-testid="guide-section-diff">
      <div
        className="flex min-w-0 items-center gap-2 px-4 pt-4 pb-2 md:px-5"
        data-testid="guide-section-back-row"
      >
        <Button
          variant="ghost"
          size="sm"
          className="shrink-0 gap-1.5 px-2 font-normal"
          onClick={onBack}
          data-testid="guide-section-back"
        >
          <UiBackIcon className="size-4" />
          {GUIDE_FACE_LABEL}
        </Button>
        <span aria-hidden className="h-4 w-px shrink-0 bg-glass-stroke" />
        {page.caption && (
          <span
            className="shrink-0 font-mono text-xs tabular-nums text-muted-foreground"
            data-testid="guide-section-caption"
          >
            {page.caption}
          </span>
        )}
        <span className="min-w-0 flex-1 truncate text-sm font-medium">{page.title}</span>
        <span
          className="flex shrink-0 items-center gap-1 text-xs text-muted-foreground tabular-nums"
          title={guideSectionSummary(page)}
        >
          <DiffCounts additions={page.additions} deletions={page.deletions} className="text-xs" />
          {!isMobile && <span>{`· ${guideFileCountLabel(page.files.length)}`}</span>}
        </span>
      </div>
      <div className={cn(`flex min-w-0 gap-3 md:px-5`)}>
        {!isMobile && page.files.length > 1 && (
          <aside
            className="sticky top-0 hidden shrink-0 self-start md:block"
            style={{ width: GUIDE_SECTION_TREE_WIDTH }}
            data-testid="guide-section-tree"
          >
            <FileDiffTree files={page.files} selected={selected} onSelect={onSelect} />
          </aside>
        )}
        <FileDiffList
          files={page.files}
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
