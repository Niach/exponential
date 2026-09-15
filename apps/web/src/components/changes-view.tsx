import { FileDiffList, Pill } from "@exp/ui"
import type { DiffFile } from "@exp/domain-contract/diff"
import { DIFF_SCOPE_ALL_LABEL } from "@/lib/session-file-cards"

// EXP-895: the CHANGES face — `@exp/ui`'s `FileDiffList` plus the one thing the
// app owns around it, the turn-scope chip. EXP-877 made the run's changes a
// FACE of the work tab (the full 896px column under the same header, in place
// of the transcript); the review page and an issue's Changes face draw the very
// same body, differing only in `nav`.
//
// A file card row in the transcript opens this face already scrolled to its
// file and SCOPED to that turn (EXP-862); the chip widens it back to the whole
// run.

export function ChangesView({
  files,
  nav,
  selected = null,
  onSelect,
  defaultCollapsed = false,
  density,
  truncatedLines,
  emptyLabel,
  scopeLabel = null,
  onClearScope,
}: {
  files: readonly DiffFile[]
  /** `auto` = the md+ file column beside the cards; `none` = cards only (a
   *  phone's file sheet or a tool card owns the list). */
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
  /** EXP-862: ONE turn's files, not the whole run — the chip says which
   *  (`This turn: 3 files`) and clicking it returns to everything. Absent =
   *  run scope, no chip. */
  scopeLabel?: string | null
  onClearScope?: () => void
}) {
  return (
    <div className="flex flex-col gap-3" data-testid="changes-view">
      {scopeLabel && onClearScope && (
        <div className="flex items-center gap-2 px-3 pt-3 text-xs text-muted-foreground">
          <span>{scopeLabel}</span>
          <Pill
            size="sm"
            mode="action"
            className="shrink-0"
            onClick={onClearScope}
            aria-label={DIFF_SCOPE_ALL_LABEL}
            title={DIFF_SCOPE_ALL_LABEL}
            data-testid="changes-scope-chip"
          >
            {DIFF_SCOPE_ALL_LABEL}
          </Pill>
        </div>
      )}
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
