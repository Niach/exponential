import { forwardRef, useState, type CSSProperties } from "react"
import { Maximize2 } from "lucide-react"
import { isVideoContentType } from "@/lib/storage/issue-attachments"
import { buildAttachmentPosterUrl } from "@/lib/storage/issue-attachments"
import { formatDuration } from "@/lib/storage/video-metadata"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

/** The subset of an `attachments` row the player needs (synced shape or
 *  the tRPC storage list). */
export interface MediaAttachmentLike {
  id: string
  filename: string
  contentType: string
  url: string
  width?: number | null
  height?: number | null
  durationMs?: number | null
  posterStorageKey?: string | null
}

interface AttachmentMediaPlayerProps {
  attachment: MediaAttachmentLike
  /** `?w=` display width (video only); undefined = natural/full column. */
  displayWidth?: number | null
  /** Shows the expand-to-lightbox affordance over a video. */
  onExpand?: () => void
  className?: string
  style?: CSSProperties
  /** Extra props the description editor spreads onto the frame (drag handle). */
  frameProps?: React.HTMLAttributes<HTMLDivElement>
}

/**
 * EXP-824: ONE inline player for every surface — the description/comment
 * media block, the comment attachment tile and the storage preview. A video
 * sits in a frame whose `aspect-ratio` comes from the probed dimensions, so
 * the column never jumps while the poster/metadata load; the duration chip
 * hides on first play. Audio is the bare browser control under the filename.
 */
export const AttachmentMediaPlayer = forwardRef<
  HTMLVideoElement,
  AttachmentMediaPlayerProps
>(function AttachmentMediaPlayer(
  { attachment, displayWidth, onExpand, className, style, frameProps },
  ref
) {
  const [started, setStarted] = useState(false)
  const [loadedDurationMs, setLoadedDurationMs] = useState<number | null>(null)
  const durationMs = attachment.durationMs ?? loadedDurationMs
  const isVideo = isVideoContentType(attachment.contentType)

  if (!isVideo) {
    return (
      <div
        {...frameProps}
        className={cn(`editor-audio`, className, frameProps?.className)}
        style={style}
      >
        <div className="editor-audio-meta">
          <span className="min-w-0 truncate text-xs text-foreground/80">
            {attachment.filename}
          </span>
          {durationMs !== null && durationMs > 0 ? (
            <span className="shrink-0 text-[10px] text-muted-foreground tabular-nums">
              {formatDuration(durationMs)}
            </span>
          ) : null}
        </div>
        <audio
          controls
          preload="metadata"
          src={attachment.url}
          className="editor-audio-control"
          onLoadedMetadata={(event) => {
            const seconds = event.currentTarget.duration
            if (Number.isFinite(seconds)) setLoadedDurationMs(seconds * 1000)
          }}
        />
      </div>
    )
  }

  const width = typeof attachment.width === `number` ? attachment.width : undefined
  const height =
    typeof attachment.height === `number` ? attachment.height : undefined
  const poster = attachment.posterStorageKey
    ? buildAttachmentPosterUrl(attachment.id)
    : undefined

  return (
    <div
      {...frameProps}
      className={cn(`editor-media-frame`, className, frameProps?.className)}
      style={{
        ...(width && height ? { aspectRatio: `${width} / ${height}` } : undefined),
        ...(displayWidth ? { width: `${displayWidth}px` } : undefined),
        ...style,
      }}
    >
      <video
        ref={ref}
        controls
        preload="metadata"
        playsInline
        src={attachment.url}
        poster={poster}
        width={width}
        height={height}
        className="editor-media"
        onPlay={() => setStarted(true)}
        onLoadedMetadata={(event) => {
          const seconds = event.currentTarget.duration
          if (Number.isFinite(seconds)) setLoadedDurationMs(seconds * 1000)
        }}
      />
      {!started && durationMs !== null && durationMs > 0 ? (
        <span className="editor-media-duration" aria-hidden="true">
          {formatDuration(durationMs)}
        </span>
      ) : null}
      {onExpand ? (
        <Button
          variant="ghost"
          size="icon-xs"
          className="editor-media-expand"
          aria-label={`Open ${attachment.filename} in viewer`}
          onClick={onExpand}
        >
          <Maximize2 className="size-3.5" />
        </Button>
      ) : null}
    </div>
  )
})
