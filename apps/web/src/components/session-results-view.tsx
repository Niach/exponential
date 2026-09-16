import { useLayoutEffect, useRef, useState, type RefObject } from "react"
import { Button, GlassSectionHeader } from "@exp/ui"
import { ImagePreviewDialog } from "@/components/image-preview-dialog"
import {
  groupSessionResults,
  SESSION_RESULT_TILE_HEIGHT,
  sessionResultTileHeightFitting,
  sessionResultTileWidth,
  type SessionResultEntry,
} from "@/lib/session-results"
import { cn } from "@/lib/utils"

// EXP-879: the RESULTS face — the screenshots a run published with
// `exponential_sessions_results`, read off the synced `coding_sessions.results`
// jsonb. One group band per topic (EXP-818's `GlassSectionHeader`, the same
// band every list wears) over a wrapping strip of tiles; every tile is the
// same 320px tall, so an iOS, an Android and a web shot of one screen read as
// one row. Tapping a tile opens the shared lightbox. Desktop
// `crates/ui/src/session_results.rs`, iOS `SessionResultsView`, Android
// `SessionResultsScreen` mirror it.
//
// On a phone that 320px base makes a landscape shot 480px wide and the column
// clips it, so the page is measured ONCE and every tile shares the one
// `sessionResultTileHeightFitting` factor (the shared rule ×4) — per-row
// fitting would break the equal-height strip.

/** The measured CONTENT width of a node, 0 until the first measurement (and in
 *  jsdom, which has no layout): the callers render at the base height then. */
function useContentWidth(ref: RefObject<HTMLElement | null>): number {
  const [width, setWidth] = useState(0)
  useLayoutEffect(() => {
    const node = ref.current
    if (!node) return
    const measure = () => {
      const style = getComputedStyle(node)
      const padding =
        (Number.parseFloat(style.paddingLeft) || 0) +
        (Number.parseFloat(style.paddingRight) || 0)
      setWidth(Math.max(0, node.getBoundingClientRect().width - padding))
    }
    measure()
    if (typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(measure)
    observer.observe(node)
    return () => observer.disconnect()
  }, [ref])
  return width
}

function ResultTile({
  entry,
  height,
  onOpen,
}: {
  entry: SessionResultEntry
  height: number
  onOpen: () => void
}) {
  return (
    <Button
      variant="ghost"
      onClick={onOpen}
      title={entry.label}
      data-testid={`session-result-${entry.attachmentId}`}
      className={cn(
        `h-auto w-auto flex-col items-start gap-1.5 rounded-lg p-0`,
        `hover:bg-transparent`
      )}
    >
      <img
        src={`/api/attachments/${entry.attachmentId}`}
        alt={entry.label}
        loading="lazy"
        // ONE height for the whole page (the 320px base, or the fitted one on
        // a narrow column — never an `h-[320px]` literal that would drift from
        // the constant), the probed aspect for the width: the tile keeps the
        // shot's shape and a 4:3 desktop frame stands in when the upload could
        // not be measured (`sessionResultTileWidth`).
        style={{
          height,
          aspectRatio: `${sessionResultTileWidth(entry, height)} / ${height}`,
        }}
        className="w-auto rounded-lg border border-glass-stroke-card object-cover"
      />
      <span className="w-full truncate text-xs text-muted-foreground">
        {entry.label}
      </span>
    </Button>
  )
}

export function SessionResultsView({
  results,
}: {
  results: readonly SessionResultEntry[]
}) {
  const [preview, setPreview] = useState<SessionResultEntry | null>(null)
  const groups = groupSessionResults(results)
  // Every band's wrapping strip sits in the SAME column, so one measurement
  // (the root's content width) fits the whole page.
  const containerRef = useRef<HTMLDivElement>(null)
  const width = useContentWidth(containerRef)
  const height = width
    ? sessionResultTileHeightFitting(results, width)
    : SESSION_RESULT_TILE_HEIGHT
  return (
    <div
      ref={containerRef}
      className="flex flex-col gap-4 py-3"
      data-testid="session-results"
    >
      {groups.map((group) => (
        <div key={group.topic} className="flex flex-col">
          <GlassSectionHeader label={group.topic} />
          <div className="flex flex-wrap gap-3">
            {group.entries.map((entry) => (
              <ResultTile
                key={`${entry.topic}/${entry.label}/${entry.attachmentId}`}
                entry={entry}
                height={height}
                onOpen={() => setPreview(entry)}
              />
            ))}
          </div>
        </div>
      ))}
      {preview && (
        <ImagePreviewDialog
          open
          onOpenChange={(open) => {
            if (!open) setPreview(null)
          }}
          src={`/api/attachments/${preview.attachmentId}`}
          alt={preview.label}
          label={preview.label}
        />
      )}
    </div>
  )
}
