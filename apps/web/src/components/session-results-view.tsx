import { useState } from "react"
import { Button, GlassSectionHeader } from "@exp/ui"
import { ImagePreviewDialog } from "@/components/image-preview-dialog"
import {
  groupSessionResults,
  SESSION_RESULT_TILE_HEIGHT,
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

function ResultTile({
  entry,
  onOpen,
}: {
  entry: SessionResultEntry
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
        // ONE height (`SESSION_RESULT_TILE_HEIGHT`, 320px — the constant, not
        // an `h-[320px]` literal that would drift from it), the probed aspect
        // for the width: the tile keeps the shot's shape and a 4:3 desktop
        // frame stands in when the upload could not be measured
        // (`sessionResultTileWidth`).
        style={{
          height: SESSION_RESULT_TILE_HEIGHT,
          aspectRatio: `${sessionResultTileWidth(entry)} / ${SESSION_RESULT_TILE_HEIGHT}`,
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
  return (
    <div className="flex flex-col gap-4 py-3" data-testid="session-results">
      {groups.map((group) => (
        <div key={group.topic} className="flex flex-col">
          <GlassSectionHeader label={group.topic} />
          <div className="flex flex-wrap gap-3">
            {group.entries.map((entry) => (
              <ResultTile
                key={`${entry.topic}/${entry.label}/${entry.attachmentId}`}
                entry={entry}
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
