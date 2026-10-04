import { conceptIcon, FileDiffTree } from "@exp/ui"
import { useReviewFilesSlot } from "@/lib/review-files-slot"
import { SidebarBackRow } from "@/components/team/sidebar-back-row"

// EXP-916: the sidebar's panel beside a REVIEW — the pull request's file
// tree, where every other detail keeps the list it came from (`TeamListNav`).
// A review's context is the files it touches, not the queue: the same
// `FileDiffTree` the page used to draw beside its cards, now in the 17rem
// panel slot under the fixed header (EXP-870), with the same back row the
// list nav wears, aimed at the face the diff was opened from (EXP-1154: the
// issue's own face, a run's Run face; the review page is gone). The
// desktop's `ReviewFilesNav` is the twin (`LeftOccupant::ReviewFiles`).
//
// The files are the page's (`review-files-slot.ts`): it fetches them and
// owns the selection, so a pick here scrolls ITS diff — this panel holds no
// state of its own beyond the tree's folds and filter.

const UiLoadingIcon = conceptIcon(`ui-loading`)

export function ReviewFilesNav() {
  const slot = useReviewFilesSlot()
  return (
    <>
      {/* EXP-945: every publisher names its own way back. */}
      {slot && (
        <SidebarBackRow label={slot.back.label} onBack={slot.back.onBack} />
      )}
      <div
        className="flex min-h-0 flex-1 flex-col"
        data-testid="review-files-nav"
      >
        {slot === null || slot.status === `loading` ? (
          <div className="flex items-center gap-2 px-3 py-3 text-xs text-muted-foreground">
            <UiLoadingIcon className="size-3.5 animate-spin" /> Loading changes…
          </div>
        ) : slot.status === `error` ? (
          <div className="px-3 py-3 text-xs text-destructive">
            Couldn’t load changes.
          </div>
        ) : slot.files.length === 0 ? (
          <div className="px-3 py-3 text-xs text-muted-foreground">
            No changes yet.
          </div>
        ) : (
          <FileDiffTree
            // Re-keyed per subject so the folds and the filter start fresh.
            key={slot.subjectId}
            files={slot.files}
            selected={slot.selected}
            onSelect={slot.onSelect}
            flush
          />
        )}
      </div>
    </>
  )
}
