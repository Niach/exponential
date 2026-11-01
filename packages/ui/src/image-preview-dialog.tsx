import { useRef, useState, type CSSProperties, type KeyboardEvent, type ReactNode, type TouchEvent } from "react"
import { Dialog, DialogContent, DialogTitle } from "./dialog"
import { SESSION_RESULT_TALL_ASPECT } from "./session-results"
import { ZoomableImage } from "./zoomable-image"

export type PreviewMediaKind = `image` | `video` | `audio`

export interface PreviewNaturalSize {
  width: number
  height: number
}

/** EXP-1128: a picture taller than 3× its width (a full-page capture) would
 *  fit the viewport's height as a sliver, so the lightbox opens it at full
 *  width, at its top, and the wheel scrolls it (EXP-1586: the zoom view's
 *  fit-width rest). ONE threshold ×4, the Results
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
   *  tall one (`isTallPreview`) opens fit-to-width at its top. */
  naturalSize?: PreviewNaturalSize | null
  /** EXP-1586: a settled zoom change of the picture. */
  onZoom?: (zoom: { scale: number; atFit: boolean }) => void
}

interface ImagePreviewDialogProps extends Omit<PreviewMediaProps, `onZoom`> {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** EXP-1283: a side PANEL (the Guide picture viewer's topic, label, text
   *  and pager): wide = the picture left, a fixed 360px panel right; phones
   *  = a full-screen page, the picture on top, the panel below. */
  panel?: ReactNode
  /** ←/→ (and a phone swipe at rest) step through a set. */
  onPrev?: () => void
  onNext?: () => void
}

/** A horizontal swipe this far (px) steps the set on a phone. */
const SWIPE_PX = 48

/** The keys the zoom view answers (+ = − 0 1); the dialog forwards them to
 *  it from wherever the focus sits inside the dialog. */
const ZOOM_KEYS = new Set([`+`, `=`, `-`, `−`, `_`, `0`, `1`])

function knownSize(size: PreviewNaturalSize | null | undefined): PreviewNaturalSize | null {
  return size && size.width > 0 && size.height > 0 ? size : null
}

/**
 * EXP-1586: the bare lightbox's picture box, the picture's fitted size within
 * min(96vw, 80rem) × 85vh (less the dialog's padding), never upscaled. A tall
 * picture keeps the reading column (≤ 60rem) at the full height: it opens
 * fit-width at its top and the wheel scrolls it.
 */
export function previewBoxStyle(natural: PreviewNaturalSize): CSSProperties {
  const { width: w, height: h } = natural
  if (isTallPreview(natural)) {
    const width = `min(calc(96vw - 1rem), 59rem, ${w}px)`
    return { width, height: `min(calc(85vh - 1rem), calc(${width} * ${h / w}))` }
  }
  const width = `min(calc(96vw - 1rem), 79rem, ${w}px, calc((85vh - 1rem) * ${w / h}))`
  return { width, height: `calc(${width} * ${h / w})` }
}

/** Below `sm` the zoom view fills the full-screen page (beats the inline box). */
const PHONE_PAGE_ZOOM = `max-sm:min-h-0 max-sm:flex-1 max-sm:h-full! max-sm:w-full! max-sm:rounded-none`

/** The box of a picture whose size never resolved (it failed to load). */
const UNKNOWN_BOX: CSSProperties = { width: `min(calc(96vw - 1rem), 48rem)` }

/**
 * The bare lightbox's picture: THE zoom view in a box hugging the picture.
 * Without a caller-known size it probes the picture first (a hidden <img>,
 * the dialog's loading look) so the box never jumps after it shows.
 */
function PreviewPicture({
  src,
  alt,
  naturalSize,
  onZoom,
}: Pick<PreviewMediaProps, `src` | `alt` | `naturalSize` | `onZoom`>) {
  const [probed, setProbed] = useState<{ src: string; size: PreviewNaturalSize | null } | null>(null)
  const given = knownSize(naturalSize)
  const size = given ?? (probed?.src === src ? probed.size : undefined)
  if (size === undefined) {
    return (
      <img
        key={src}
        src={src}
        alt={alt}
        hidden
        onLoad={(event) => {
          const img = event.currentTarget
          setProbed({ src, size: knownSize({ width: img.naturalWidth, height: img.naturalHeight }) })
        }}
        onError={() => setProbed({ src, size: null })}
      />
    )
  }
  return (
    <ZoomableImage
      data-testid="preview-zoom"
      src={src}
      alt={alt ?? ``}
      naturalWidth={size?.width}
      naturalHeight={size?.height}
      style={size ? previewBoxStyle(size) : UNKNOWN_BOX}
      // Phones: the lightbox is a full-screen page, so the zoom view IS the
      // page (the picture still rests fitted and centred; a zoom fills the
      // screen like the native viewers, not the fitted box).
      className={PHONE_PAGE_ZOOM}
      onZoom={onZoom}
    />
  )
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
  onZoom,
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
  return <PreviewPicture src={src} alt={alt} naturalSize={naturalSize} onZoom={onZoom} />
}

/** A field the zoom keys must not be taken from. */
function isEditable(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))
  )
}

/**
 * The shared image lightbox (EXP-316): a borderless dialog hugging the image.
 * Used by the description editor's image node view and the storage table's
 * filename preview. EXP-824: a video attachment opens here too, autoplaying
 * with the browser controls. EXP-1586: a picture is THE zoom view (pinch,
 * ⌘/ctrl-wheel, double-click, + − 0 1, the zoom pill).
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
  panel,
  onPrev,
  onNext,
}: ImagePreviewDialogProps) {
  const contentRef = useRef<HTMLDivElement | null>(null)
  const swipeFrom = useRef<{ x: number; y: number } | null>(null)
  // The picture's last settled zoom; a new picture opens at rest.
  const zoom = useRef({ src, atFit: true })
  const onZoom = ({ atFit }: { atFit: boolean }) => {
    zoom.current = { src, atFit }
  }
  const atRest = () => zoom.current.src !== src || zoom.current.atFit
  // ←/→ step the set in BOTH presentations (the panel one and the bare
  // lightbox), zoomed or not; + = − 0 1 reach the zoom view from anywhere
  // in the dialog.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === `ArrowLeft` && onPrev) {
      event.preventDefault()
      onPrev()
    } else if (event.key === `ArrowRight` && onNext) {
      event.preventDefault()
      onNext()
    } else if (ZOOM_KEYS.has(event.key) && !event.metaKey && !event.ctrlKey && !event.altKey) {
      const view = contentRef.current?.querySelector<HTMLElement>(`[data-slot="zoomable-image"]`)
      if (!view || view.contains(event.target as Node) || isEditable(event.target)) return
      const forwarded = new window.KeyboardEvent(`keydown`, { key: event.key, cancelable: true })
      if (!view.dispatchEvent(forwarded)) event.preventDefault()
    }
  }
  // The keys need the focus inside the dialog: the dialog itself takes it
  // (not the view, whose focus ring would light up on open).
  const onOpenAutoFocus = (event: Event) => {
    event.preventDefault()
    contentRef.current?.focus()
  }
  // A phone swipe steps the set only while the picture is at rest (a zoomed
  // picture pans instead), only when it is mostly horizontal (|dx| > |dy|,
  // so a vertical scroll never flips the page), and never from a pinch.
  const onTouchStart = (event: TouchEvent<HTMLDivElement>) => {
    const touch = event.touches.length === 1 ? event.touches[0] : undefined
    swipeFrom.current = touch ? { x: touch.clientX, y: touch.clientY } : null
  }
  const onTouchEnd = (event: TouchEvent<HTMLDivElement>) => {
    const from = swipeFrom.current
    swipeFrom.current = null
    const touch = event.changedTouches[0]
    if (from === null || !touch || !atRest()) return
    const dx = touch.clientX - from.x
    const dy = touch.clientY - from.y
    if (Math.abs(dx) <= Math.abs(dy)) return
    if (dx > SWIPE_PX) onPrev?.()
    else if (-dx > SWIPE_PX) onNext?.()
  }
  if (panel !== undefined) {
    return (
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent
          ref={contentRef}
          mobile="page"
          data-testid="image-preview-panel-dialog"
          // The `page` arm re-states `max-sm:p-6 sm:p-6`, which an unprefixed
          // `p-0` loses to: zero the padding per breakpoint.
          className="gap-0 max-sm:p-0 sm:p-0 max-sm:overflow-y-auto sm:h-[min(85vh,52rem)] sm:w-[min(96vw,80rem)] sm:max-w-[min(96vw,80rem)] sm:flex-row"
          aria-describedby={undefined}
          onKeyDown={onKeyDown}
          onOpenAutoFocus={onOpenAutoFocus}
        >
          <DialogTitle className="sr-only">{label}</DialogTitle>
          <div
            data-testid="image-preview-media"
            // A fixed box the zoom view fills: the flex half beside the panel
            // (sm+), 60dvh of picture on a phone page.
            className="flex min-h-0 min-w-0 items-center justify-center bg-black/30 p-4 max-sm:h-[calc(60dvh+4.5rem+env(safe-area-inset-top))] max-sm:shrink-0 max-sm:pt-[calc(3.5rem+env(safe-area-inset-top))] sm:flex-1"
            onTouchStart={onTouchStart}
            onTouchEnd={onTouchEnd}
          >
            <ZoomableImage
              data-testid="preview-zoom"
              src={src}
              alt={alt ?? ``}
              naturalWidth={knownSize(naturalSize)?.width}
              naturalHeight={knownSize(naturalSize)?.height}
              style={{ height: `100%` }}
              onZoom={onZoom}
            />
          </div>
          <div
            data-testid="image-preview-side"
            className="flex min-h-0 flex-col border-glass-stroke max-sm:flex-1 sm:w-[360px] sm:shrink-0 sm:border-l"
          >
            {panel}
          </div>
        </DialogContent>
      </Dialog>
    )
  }
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        ref={contentRef}
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
        onKeyDown={onKeyDown}
        onOpenAutoFocus={onOpenAutoFocus}
        onTouchStart={onTouchStart}
        onTouchEnd={onTouchEnd}
      >
        <DialogTitle className="sr-only">{label}</DialogTitle>
        <PreviewMedia
          src={src}
          alt={alt}
          label={label}
          kind={kind}
          poster={poster}
          naturalSize={naturalSize}
          onZoom={onZoom}
        />
      </DialogContent>
    </Dialog>
  )
}
