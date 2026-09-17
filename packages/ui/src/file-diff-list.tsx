import { useEffect, useMemo, useRef, useState } from "react"
import type { DiffFile } from "@exp/domain-contract/diff"

import { cn } from "./cn"
import { diffOpensByDefault, FileDiffCard, type DiffDensity } from "./file-diff-card"
import { FileDiffTree } from "./file-diff-tree"
import { useIsMobile } from "./use-mobile"

// EXP-895 — the ONE diff view. `DiffFile[]` in, the file column beside the
// cards, nothing else: the review page, the run's Changes face, an issue's
// Changes face and a single tool call's patch are all THIS, differing only in
// `nav`, `density` and whether the cards start collapsed.
//
// The aside is md-and-up only. On a phone the file list is a SHEET off the work
// bar's leading slot (apps/web `changes-file-sheet.tsx`), because a 64-wide
// column beside a diff leaves neither readable.
//
// EXP-916: the aside is the file TREE and it does not fold — a diff HAS a file
// column, and a chevron that takes it away was one control over a surface
// whose whole job is the list beside the cards.

/** EXP-786: the note under a publisher-CUT diff — the lines it dropped. */
export function truncatedLinesNote(lines: number): string {
  return `${lines} more line${lines === 1 ? `` : `s`} truncated`
}

export function FileDiffList({
  files,
  nav = `auto`,
  defaultCollapsed = false,
  density = `comfortable`,
  focusPath = null,
  onSelect,
  truncatedLines,
  emptyLabel,
  className,
}: {
  files: readonly DiffFile[]
  /** `auto` = the md+ file column; `none` = cards only (a sheet, a top bar or
   *  a tool card owns the list instead). */
  nav?: `auto` | `none`
  /** Every card starts closed, whatever its size. */
  defaultCollapsed?: boolean
  density?: DiffDensity
  /** Open this file and scroll to it — whatever the reader picked (a nav row, a
   *  turn's file card). A CHANGE scrolls; re-picking the file in view is a
   *  no-op. */
  focusPath?: string | null
  /** A pick in the file COLUMN, reported up so a caller holding the selection
   *  (the run's Changes face) stays in step. The list always jumps on its own;
   *  this is notification, not permission. */
  onSelect?: (path: string) => void
  /** The publisher's own dropped-line count (`Diff.truncatedLines`). */
  truncatedLines?: number | null
  /** What an EMPTY file set says. Absent = nothing is drawn. */
  emptyLabel?: string
  className?: string
}) {
  // Sparse user overrides on top of the size-based defaults, keyed by path — a
  // refresh replaces `files` without discarding the reader's toggles.
  const [overrides, setOverrides] = useState<Record<string, boolean>>({})
  const sectionRefs = useRef(new Map<string, HTMLDivElement>())
  // ONE matchMedia subscription for the whole list — the path labels below only
  // need the answer (EXP-698).
  const isMobile = useIsMobile()

  const defaults = useMemo(() => {
    const map = new Map<string, boolean>()
    for (const file of files) {
      map.set(file.path, diffOpensByDefault(file, defaultCollapsed))
    }
    return map
  }, [files, defaultCollapsed])

  const jumpTo = (path: string) => {
    setOverrides((prev) => ({ ...prev, [path]: true }))
    // Scroll on the next frame so a just-expanded section has laid out.
    requestAnimationFrame(() => {
      sectionRefs.current
        .get(path)
        ?.scrollIntoView({ behavior: `smooth`, block: `start` })
    })
  }

  // The caller's own selection: the same jump, driven from outside.
  const jumpRef = useRef(jumpTo)
  jumpRef.current = jumpTo
  useEffect(() => {
    if (!focusPath) return
    jumpRef.current(focusPath)
  }, [focusPath])

  const showNav = nav === `auto` && files.length > 1

  return (
    <div
      className={cn(`flex min-w-0 gap-3 p-3`, className)}
      data-testid="file-diff-list"
    >
      {showNav && (
        <aside className="hidden w-64 shrink-0 flex-col gap-2 md:flex">
          <FileDiffTree
            files={files}
            selected={focusPath}
            onSelect={(path) => {
              jumpTo(path)
              onSelect?.(path)
            }}
          />
        </aside>
      )}
      <div className="min-w-0 flex-1 space-y-2">
        {files.length === 0 && emptyLabel ? (
          <p className="px-1 py-2 text-xs text-muted-foreground">{emptyLabel}</p>
        ) : null}
        {files.map((file) => (
          <div
            key={file.path}
            ref={(el) => {
              if (el) sectionRefs.current.set(file.path, el)
              else sectionRefs.current.delete(file.path)
            }}
            className="scroll-mt-2"
          >
            <FileDiffCard
              file={file}
              isMobile={isMobile}
              density={density}
              open={overrides[file.path] ?? defaults.get(file.path) ?? true}
              onOpenChange={(open) =>
                setOverrides((prev) => ({ ...prev, [file.path]: open }))
              }
            />
          </div>
        ))}
        {truncatedLines != null && truncatedLines > 0 && (
          <p
            className="px-1 text-[0.6875rem] text-muted-foreground/70"
            data-testid="diff-truncation-note"
          >
            {truncatedLinesNote(truncatedLines)}
          </p>
        )}
      </div>
    </div>
  )
}
