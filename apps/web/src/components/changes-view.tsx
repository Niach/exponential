import { FileDiffList } from "@exp/ui"
import type { DiffFile } from "@exp/domain-contract/diff"

// EXP-895/EXP-916: the CHANGES face — a thin wrapper over `@exp/ui`'s
// `FileDiffList`. The review page, the run's Changes face and an issue's
// Changes face all draw THIS, differing only in `nav`. Nothing sits above the
// cards any more: the merge, the PR state and GitHub live in the surface's own
// header (the work header on a run, `ChangesTopBar` on Reviews).

export function ChangesView({
  files,
  nav,
  selected = null,
  onSelect,
  defaultCollapsed = false,
  density,
  truncatedLines,
  emptyLabel,
}: {
  files: readonly DiffFile[]
  /** `auto` = the md+ file tree beside the cards; `none` = cards only (a
   *  phone's file sheet owns the list). */
  nav: `auto` | `none`
  /** The file the card list is scrolled to; null = the top. */
  selected?: string | null
  /** A pick in the file column (or the phone's sheet) — the caller holds the
   *  selection so the two stay in step. */
  onSelect?: (path: string) => void
  defaultCollapsed?: boolean
  density?: `comfortable` | `compact`
  truncatedLines?: number | null
  emptyLabel?: string
}) {
  return (
    <div className="flex flex-col gap-3" data-testid="changes-view">
      <FileDiffList
        files={files}
        nav={nav}
        defaultCollapsed={defaultCollapsed}
        density={density}
        focusPath={selected}
        onSelect={onSelect}
        truncatedLines={truncatedLines}
        emptyLabel={emptyLabel}
      />
    </div>
  )
}
