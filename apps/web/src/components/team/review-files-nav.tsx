import { useNavigate } from "@tanstack/react-router"
import { conceptIcon, FileDiffTree } from "@exp/ui"
import { useReviewFilesSlot } from "@/lib/review-files-slot"
import { SidebarBackRow } from "@/components/team/sidebar-back-row"

// EXP-916: the sidebar's panel beside a REVIEW — the pull request's file
// tree, where every other detail keeps the list it came from (`TeamListNav`).
// A review's context is the files it touches, not the queue: the same
// `FileDiffTree` the page used to draw beside its cards, now in the 17rem
// panel slot under the fixed header (EXP-870), with the same back row the
// list nav wears, aimed at Reviews. The desktop's `ReviewFilesNav` is the
// twin (`LeftOccupant::ReviewFiles`).
//
// The files are the page's (`review-files-slot.ts`): it fetches them and
// owns the selection, so a pick here scrolls ITS diff — this panel holds no
// state of its own beyond the tree's folds and filter.

const UiLoadingIcon = conceptIcon(`ui-loading`)

export function ReviewFilesNav({ teamSlug }: { teamSlug: string }) {
  const navigate = useNavigate()
  const slot = useReviewFilesSlot()
  return (
    <>
      <SidebarBackRow
        label="Reviews"
        onBack={() =>
          void navigate({ to: `/t/$teamSlug/reviews`, params: { teamSlug } })
        }
      />
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
            // Re-keyed per review so the folds and the filter start fresh.
            key={slot.issueId}
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
