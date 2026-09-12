import { useCallback, useEffect, useRef, useState } from "react"
import { conceptIcon } from "@/lib/icons.generated"
import { AddDelCounts, FileDiffList, FileNav } from "@/components/diff-view"
import type { PullFile } from "@/components/diff-view"
import { useIsMobile } from "@/hooks/use-mobile"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

// EXP-850 §11: the session's changes as a PANE beside the transcript, not a
// bar under it. The bottom "Changes" strip is gone on md+ (the phone keeps
// its floating sheet): the diff splits the session column, the reader picks a
// file in the collapsible list and the patch list scrolls to it, and the pane
// remembers its width per session.
//
// Icons are CONCEPTS — the desktop IDE draws the same pane.
const CodingDiffIcon = conceptIcon(`coding-diff`)
const UiCloseIcon = conceptIcon(`ui-close`)
const UiChecklistIcon = conceptIcon(`ui-checklist`)

/** The pane opens at 45% of the session column and never narrows past this. */
export const DIFF_PANE_MIN_WIDTH = 360
export const DIFF_PANE_DEFAULT_FRACTION = 0.45

const WIDTH_KEY_PREFIX = `exp.session-diff-width.`

/** The remembered width for a session, or null — a bad or absent entry (and
 *  any storage the browser refuses) reads as "no preference". */
export function readDiffPaneWidth(
  sessionId: string,
  storage?: Pick<Storage, `getItem`>
): number | null {
  try {
    const raw = (storage ?? window.localStorage).getItem(
      `${WIDTH_KEY_PREFIX}${sessionId}`
    )
    if (!raw) return null
    const width = Number.parseInt(raw, 10)
    return Number.isFinite(width) && width >= DIFF_PANE_MIN_WIDTH ? width : null
  } catch {
    return null
  }
}

export function writeDiffPaneWidth(
  sessionId: string,
  width: number,
  storage?: Pick<Storage, `setItem`>
): void {
  try {
    ;(storage ?? window.localStorage).setItem(
      `${WIDTH_KEY_PREFIX}${sessionId}`,
      String(Math.round(width))
    )
  } catch {
    // A private window with storage denied: the pane just forgets.
  }
}

/** The width a drag lands on: never under the minimum, never wider than the
 *  column minus one minimum for the transcript. */
export function clampDiffPaneWidth(width: number, columnWidth: number): number {
  const max = Math.max(DIFF_PANE_MIN_WIDTH, columnWidth - DIFF_PANE_MIN_WIDTH)
  return Math.min(Math.max(width, DIFF_PANE_MIN_WIDTH), max)
}

export function SessionDiffPane({
  sessionId,
  files,
  selected,
  onSelect,
  onClose,
  className,
}: {
  sessionId: string
  files: PullFile[]
  /** The file the header names and the list is scrolled to; null = the top. */
  selected: string | null
  onSelect: (filename: string) => void
  onClose: () => void
  className?: string
}) {
  const isMobile = useIsMobile()
  const [width, setWidth] = useState<number | null>(null)
  const [listOpen, setListOpen] = useState(false)
  const paneRef = useRef<HTMLDivElement | null>(null)
  const dragRef = useRef<{ startX: number; startWidth: number } | null>(null)

  useEffect(() => {
    setWidth(readDiffPaneWidth(sessionId))
  }, [sessionId])

  const onPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const pane = paneRef.current
      if (!pane) return
      event.preventDefault()
      event.currentTarget.setPointerCapture(event.pointerId)
      dragRef.current = {
        startX: event.clientX,
        startWidth: pane.getBoundingClientRect().width,
      }
    },
    []
  )

  const onPointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const drag = dragRef.current
      const pane = paneRef.current
      if (!drag || !pane) return
      const column = pane.parentElement?.getBoundingClientRect().width ?? 0
      // The handle sits on the pane's LEFT edge, so dragging left widens it.
      const next = clampDiffPaneWidth(
        drag.startWidth + (drag.startX - event.clientX),
        column
      )
      setWidth(next)
    },
    []
  )

  const onPointerUp = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (!dragRef.current) return
      dragRef.current = null
      event.currentTarget.releasePointerCapture(event.pointerId)
      const pane = paneRef.current
      if (pane) writeDiffPaneWidth(sessionId, pane.getBoundingClientRect().width)
    },
    [sessionId]
  )

  const current = selected ? files.find((f) => f.filename === selected) : undefined
  const totals = files.reduce(
    (acc, f) => ({
      additions: acc.additions + f.additions,
      deletions: acc.deletions + f.deletions,
    }),
    { additions: 0, deletions: 0 }
  )
  const counts = current ?? totals

  return (
    <div
      ref={paneRef}
      data-testid="session-diff-pane"
      className={cn(
        `relative flex min-w-0 shrink-0 flex-col border-l border-border bg-card/40`,
        className
      )}
      style={{
        width: width ?? `${DIFF_PANE_DEFAULT_FRACTION * 100}%`,
        minWidth: DIFF_PANE_MIN_WIDTH,
      }}
    >
      {/* The drag handle: a hairline-wide strip on the pane's own left edge. */}
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize the changes pane"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        className="absolute inset-y-0 -left-1 z-10 w-2 cursor-col-resize"
      />
      <div className="flex shrink-0 items-center gap-2 border-b border-border px-2 py-1.5 text-xs">
        {files.length > 1 && (
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={listOpen ? `Hide the file list` : `Show the file list`}
            title={listOpen ? `Hide the file list` : `Show the file list`}
            aria-pressed={listOpen}
            className="shrink-0 text-muted-foreground"
            onClick={() => setListOpen((open) => !open)}
          >
            <UiChecklistIcon />
          </Button>
        )}
        <CodingDiffIcon className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate font-mono" title={selected ?? undefined}>
          {selected ?? `Changes`}
        </span>
        <AddDelCounts
          additions={counts.additions}
          deletions={counts.deletions}
        />
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="Close the changes pane"
          title="Close the changes pane"
          className="shrink-0 text-muted-foreground"
          onClick={onClose}
        >
          <UiCloseIcon />
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
        {listOpen && files.length > 1 && (
          <div className="p-3 pb-0">
            <FileNav
              files={files}
              onJump={onSelect}
              isMobile={isMobile}
              selected={selected}
            />
          </div>
        )}
        <FileDiffList files={files} showFileNav={false} focusFile={selected} />
      </div>
    </div>
  )
}
