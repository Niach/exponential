import { Dialog, DialogContent, DialogTitle } from "./dialog"
import { SESSION_RESULT_TALL_ASPECT } from "./session-results"

export type PreviewMediaKind = `image` | `video` | `audio`

export interface PreviewNaturalSize {
  width: number
  height: number
}

/** EXP-1128: a picture taller than 3× its width (a full-page capture) would
 *  fit the viewport's height as a sliver, so the lightbox lays it out at full
 *  width and scrolls it vertically instead. ONE threshold ×4, the Results
 *  tiles' `sessionResultIsTall`. */
export function isTallPreview(
  naturalSize: PreviewNaturalSize | null | undefined
): boolean {
  return (
    !!naturalSize &&
    naturalSize.width > 0 &&
    naturalSize.height > 0 &&
    naturalSize.width / naturalSize.height < SESSION_RESULT_TALL_ASPECT
  )
}

interface PreviewMediaProps {
  src: string
  alt?: string
  // The media's name — the audio arm's visible caption, and the dialog's
  // accessible title around it.
  label: string
  /** EXP-824: the same viewer plays a clip. Defaults to an image. */
  kind?: PreviewMediaKind
  /** Poster frame shown until a video starts (video only). */
  poster?: string
  /** EXP-1128: the picture's probed pixel size when the caller knows it; a
   *  tall one (`isTallPreview`) opens fit-to-width in a vertical scroll. */
  naturalSize?: PreviewNaturalSize | null
}

interface ImagePreviewDialogProps extends PreviewMediaProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

/**
 * The lightbox's BODY on its own — the image / video / audio switch, with no
 * dialog around it, so a host that cannot run a Radix portal (the styleguide's
 * islands) still renders the specimen.
 */
export function PreviewMedia({
  src,
  alt,
  label,
  kind = `image`,
  poster,
  naturalSize,
}: PreviewMediaProps) {
  if (kind === `video`) {
    // Only mounted while open, so autoplay fires on every open and the
    // stream stops the moment the dialog unmounts.
    return (
      <video
        src={src}
        poster={poster}
        controls
        autoPlay
        playsInline
        className="max-h-[85vh] w-auto max-w-full rounded-md bg-black object-contain"
      />
    )
  }
  if (kind === `audio`) {
    return (
      <div className="flex min-w-72 flex-col gap-2 p-2">
        <span className="truncate text-sm">{label}</span>
        <audio src={src} controls autoPlay className="w-full" />
      </div>
    )
  }
  if (isTallPreview(naturalSize)) {
    // Fit to width, scroll vertically: the column is the viewport's width up
    // to 60rem and never wider than the picture itself (no upscaling), the
    // image runs its full height inside the scroller. The browser decodes
    // the one tall <img> itself; no strips needed here.
    return (
      <div
        data-testid="preview-tall-scroll"
        className="max-h-[85vh] w-[min(96vw,60rem)] overflow-y-auto overscroll-contain rounded-md"
        style={{ maxWidth: naturalSize?.width }}
      >
        <img src={src} alt={alt} className="block h-auto w-full" />
      </div>
    )
  }
  return (
    <img
      src={src}
      alt={alt}
      className="max-h-[85vh] w-auto max-w-full rounded-md object-contain"
    />
  )
}

/**
 * The shared image lightbox (EXP-316): a borderless dialog hugging the image.
 * Used by the description editor's image node view and the storage table's
 * filename preview. EXP-824: a video attachment opens here too, autoplaying
 * with the browser controls.
 */
export function ImagePreviewDialog({
  open,
  onOpenChange,
  src,
  alt,
  label,
  kind = `image`,
  poster,
  naturalSize,
}: ImagePreviewDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        // The one dialog that stays a full-screen PAGE below `sm` (EXP-687):
        // a lightbox wants the whole viewport, not a sheet.
        mobile="page"
        // sm:w-auto opts out of DialogContent's sm:w-[calc(100%-2rem)]
        // gutter width — the lightbox panel hugs the image instead of
        // spanning the viewport with the image left-aligned inside it.
        // The base panel is a flex column, so below `sm` — where it is the
        // full-screen page — the image is centred with justify/items-center
        // (the grid-era content-center/justify-items-center equivalents).
        className="w-auto max-w-[min(96vw,80rem)] p-2 max-sm:w-auto max-sm:items-center max-sm:justify-center max-sm:p-2 sm:w-auto sm:max-w-[min(96vw,80rem)]"
        aria-describedby={undefined}
      >
        <DialogTitle className="sr-only">{label}</DialogTitle>
        <PreviewMedia
          src={src}
          alt={alt}
          label={label}
          kind={kind}
          poster={poster}
          naturalSize={naturalSize}
        />
      </DialogContent>
    </Dialog>
  )
}
