import { additionsLabel, deletionsLabel, type DiffFile } from "@exp/domain-contract/diff"
import {
  Button,
  conceptIcon,
  DiffCounts,
  FileDiffList,
  FileDiffTree,
  guideFileCountLabel,
} from "@exp/ui"
import { GUIDE_FACE_LABEL, type GuideSectionPage } from "@/lib/work-faces"
import { cn } from "@/lib/utils"

// EXP-1251: a Guide section's diff as a PAGE under the Guide
// (`?view=guide&section=N&file=`; `section=all` = Show complete diff). The
// back row returns to the Guide and names the section (`02 / 06 · title`)
// with the counts it covers. md+: the file tree IN the column (216px, the
// desktop `diff_pane` width) beside the same cards every diff draws, only
// this section's files. Phones: the cards alone; the file sheet rides the
// Guide face's bar, filtered to the same files.

const UiBackIcon = conceptIcon(`ui-back`)

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
          emptyLabel="No changes in this section."
          className="min-w-0 flex-1 md:px-0"
        />
      </div>
    </div>
  )
}
