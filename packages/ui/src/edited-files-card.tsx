import { useState } from "react"
import { contract } from "@exp/domain-contract"
import type { DiffFile } from "@exp/domain-contract/diff"
import {
  editCardMoreLabel,
  EDIT_CARD_PREVIEW,
  type EditCardRow,
  type EditCardView,
} from "@exp/domain-contract/edit-card"

import { cn } from "./cn"
import { FileDiffCard } from "./file-diff-card"
import { truncatedLinesNote } from "./file-diff-list"
import { GLASS_SURFACE } from "./glass-rows"
import { conceptIcon } from "./icons.generated"

// EXP-916 — the ONE card a transcript draws for a run of consecutive file
// edits: `N files edited` over a stack of the very same `FileDiffCard` the
// review page and every Changes face use, `flush` inside the card so the rows
// DIVIDE rather than float. The grouping and the rows are the contract's
// (`@exp/domain-contract/edit-card`); this only draws them.
//
// Purely presentational: the reader's open set and its toggle belong to the
// caller (`agent-session.tsx` opens the live row and collapses the card when
// it settles; the styleguide island hands it a fixture).

const CodingDiffIcon = conceptIcon(`coding-diff`)

/** The open row's scroll box. The BOUND is the contract's
 *  `diffUi.inlineDiffMaxHeight`, read here rather than spelled as a tailwind
 *  step, so web, the natives and the desktop cap an inline patch at the same
 *  number. */
const BODY_SCROLL_CLASS = `overflow-auto overscroll-contain`

/** A `pending`/`failed` row has no patch — the card still draws its header, so
 *  it needs a file-shaped stand-in with nothing in it. */
function stubFile(row: EditCardRow): DiffFile {
  return {
    path: row.path,
    status: `modified`,
    additions: 0,
    deletions: 0,
    binary: false,
    hunks: [],
  }
}

export function EditedFilesCard({
  view,
  openPaths,
  onToggle,
  className,
}: {
  view: EditCardView
  /** The paths the reader (or the live row) has open. */
  openPaths: ReadonlySet<string>
  onToggle: (path: string) => void
  className?: string
}) {
  const [expanded, setExpanded] = useState(false)
  const more = editCardMoreLabel(view.rows.length)
  // The live row always has to be ON SCREEN: a card whose running edit sits
  // past the preview shows everything rather than hiding the one row that
  // moves — and it keeps no fold row, because there is nothing left to fold.
  const liveBeyondPreview =
    view.liveIndex !== null && view.liveIndex >= EDIT_CARD_PREVIEW
  const showAll = expanded || more === null || liveBeyondPreview
  const rows = showAll ? view.rows : view.rows.slice(0, EDIT_CARD_PREVIEW)

  return (
    <div
      className={cn(GLASS_SURFACE, `min-w-0 overflow-clip`, className)}
      data-testid="edited-files-card"
    >
      <div className="flex min-w-0 items-center gap-2 px-3 py-1.5 text-xs">
        <CodingDiffIcon className="size-3 shrink-0 text-muted-foreground/60" />
        <span className="min-w-0 truncate font-medium">{view.title}</span>
      </div>
      <div className="divide-y divide-glass-stroke border-t border-glass-stroke">
        {rows.map((row) => {
          const open = openPaths.has(row.path)
          return (
            <div
              key={row.path}
              // The body scrolls INSIDE the row (the sticky header rides its
              // own box): one 2000-line patch must never push the rest of the
              // transcript off screen.
              className={cn(`min-w-0`, open && BODY_SCROLL_CLASS)}
              style={
                open
                  ? { maxHeight: contract.diffUi.inlineDiffMaxHeight }
                  : undefined
              }
            >
              <FileDiffCard
                flush
                /* A card row holds a handful of lines and the card itself is
                   already in the transcript's flow — no observer needed. */
                eager
                density="compact"
                state={row.state}
                open={open}
                onOpenChange={() => onToggle(row.path)}
                file={row.file ?? stubFile(row)}
              />
            </div>
          )
        })}
      </div>
      {view.truncatedLines > 0 && (
        /* EXP-786: what the PUBLISHER cut off the members' patches — the same
           note a file list carries, so a card never silently shows a short
           diff. */
        <p
          className="border-t border-glass-stroke px-3 py-1 text-[0.6875rem] text-muted-foreground/70"
          data-testid="edited-files-truncation-note"
        >
          {truncatedLinesNote(view.truncatedLines)}
        </p>
      )}
      {more !== null && !liveBeyondPreview && (
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="w-full border-t border-glass-stroke px-3 py-1 text-left text-xs text-muted-foreground transition-colors duration-fast hover:bg-glass-active/50 hover:text-foreground"
          data-testid="edited-files-more"
        >
          {expanded ? contract.diffUi.showLess : more}
        </button>
      )}
    </div>
  )
}
