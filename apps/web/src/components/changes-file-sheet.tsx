import { useState } from "react"
import { contract } from "@exp/domain-contract"
import {
  conceptIcon,
  FileDiffTree,
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from "@exp/ui"
import type { DiffFile } from "@exp/domain-contract/diff"
import { MOBILE_WORK_CIRCLE_CLASS } from "@/components/mobile-work-bar"

// EXP-895: the phone's file list. A 64-wide column beside a diff leaves neither
// readable, so on a phone `FileDiffList`'s aside is gone and the list lives in a
// bottom SHEET off the work bar's LEADING slot — the same slot GitHub used to
// hold on the Changes face (GitHub moved to the header's action slot).
//
// A pick closes the sheet and reports the path; the card list scrolls to it.
// EXP-916: the sheet holds the same file TREE the md+ column does.

const NavFilesIcon = conceptIcon(`nav-files`)

export const CHANGED_FILES_TITLE = contract.diffUi.changedFilesTitle

export function ChangesFileSheet({
  files,
  selected = null,
  onSelect,
}: {
  files: readonly DiffFile[]
  selected?: string | null
  onSelect: (path: string) => void
}) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <button
        type="button"
        aria-label={CHANGED_FILES_TITLE}
        title={CHANGED_FILES_TITLE}
        data-testid="changes-file-sheet-button"
        onClick={() => setOpen(true)}
        className={MOBILE_WORK_CIRCLE_CLASS}
      >
        {/* EXP-916: Android's `FileListCircle` — an 18px white glyph over
            the count in the secondary emphasis. */}
        <span className="flex flex-col items-center gap-0.5 leading-none">
          <NavFilesIcon className="size-[18px] text-foreground" />
          <span className="text-[0.6875rem] font-medium tabular-nums">
            {files.length}
          </span>
        </span>
      </button>
      <Sheet open={open} onOpenChange={setOpen}>
        <SheetContent
          side="bottom"
          data-testid="changes-file-sheet"
          className="gap-0 p-2 pb-[max(1rem,env(safe-area-inset-bottom))]"
        >
          <SheetHeader className="px-3 pt-2 pb-1">
            <SheetTitle>{CHANGED_FILES_TITLE}</SheetTitle>
          </SheetHeader>
          <div className="min-h-0 flex-1 overflow-y-auto">
            <FileDiffTree
              files={files}
              selected={selected}
              onSelect={(path: string) => {
                setOpen(false)
                onSelect(path)
              }}
            />
          </div>
        </SheetContent>
      </Sheet>
    </>
  )
}
