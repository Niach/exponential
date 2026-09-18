import * as React from "react"

import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"

// EXP-904 — the ONE attachment thumbnail.
//
// The comment, launch and steer composers each drew a 64px thumb with a
// corner remove badge by copy-paste. `kind="video"` is the comment composer's
// arm (EXP-824): a muted, metadata-only `<video>` is the cheapest first-frame
// thumb. A NON-media file stays a chip at the call site, which reuses
// `AttachmentRemoveButton` for the same corner badge.
//
// EXP-962 — two SIZES and two ARMS. `size="tile"` (the default) is the 64px
// center-cropped tile a composer queues; `size="inline"` is the image as
// posted — natural width, capped at 480 tall, contained under the same
// hairline on the section fill — the way a comment or a steer message
// shows what it carried. `onRemove` hangs the corner badge (a pending pick,
// or a posted image its owner may delete); `onOpen` wraps the media in a
// zoom button that opens the lightbox. A thumb may carry both.
const CloseGlyph = conceptIcon(`ui-close`)

export function AttachmentRemoveButton({
  label,
  className,
  ...props
}: Omit<React.ComponentProps<`button`>, `children` | `aria-label`> & {
  label: string
}) {
  return (
    <button
      type="button"
      data-slot="attachment-remove"
      aria-label={label}
      className={cn(
        `absolute -right-1.5 -top-1.5 rounded-full border border-glass-stroke-card bg-popover p-0.5 text-muted-foreground hover:text-foreground`,
        className
      )}
      {...props}
    >
      <CloseGlyph className="size-3" />
    </button>
  )
}

const MEDIA: Record<`tile` | `inline`, string> = {
  tile: `size-16 rounded-md border border-glass-stroke-card object-cover`,
  inline: `block h-auto max-h-[480px] w-auto max-w-full rounded-lg border border-glass-stroke-card bg-glass-section object-contain`,
}

export function AttachmentThumb({
  src,
  alt = ``,
  kind = `image`,
  size = `tile`,
  width,
  height,
  removeLabel,
  onRemove,
  removeClassName,
  openLabel,
  onOpen,
  disabled,
  className,
}: {
  src: string
  alt?: string
  kind?: `image` | `video`
  /** `tile` = the 64px composer thumb; `inline` = the posted image at its
   *  own size, capped at 480 tall. */
  size?: `tile` | `inline`
  /** The probed intrinsic size (attachments carry `width`/`height`): lets
   *  the browser reserve the box before the bytes land. */
  width?: number | null
  height?: number | null
  /** The corner remove badge — rendered only with `onRemove`. */
  removeLabel?: string
  onRemove?: () => void
  /** Extra classes on the badge (a hover-only badge passes its `hidden
   *  group-hover:block` pair). */
  removeClassName?: string
  /** The zoom arm — rendered only with `onOpen`: the media becomes a button
   *  that opens the lightbox. */
  openLabel?: string
  onOpen?: () => void
  disabled?: boolean
  className?: string
}) {
  const intrinsic = width && height ? { width, height } : undefined
  const mediaClassName = MEDIA[size]
  const media =
    kind === `video` ? (
      <video
        src={src}
        muted
        playsInline
        preload="metadata"
        aria-label={alt}
        className={cn(mediaClassName, `bg-black`)}
      />
    ) : (
      <img
        src={src}
        alt={alt}
        loading={size === `inline` ? `lazy` : undefined}
        width={intrinsic?.width}
        height={intrinsic?.height}
        // `aspect-ratio` keeps the reservation correct once `max-h`/`max-w`
        // clamp one side. Rows probed before EXP-580 carry neither and
        // simply reflow on decode.
        style={
          intrinsic
            ? { aspectRatio: `${intrinsic.width} / ${intrinsic.height}` }
            : undefined
        }
        className={mediaClassName}
      />
    )
  return (
    <div
      data-slot="attachment-thumb"
      data-size={size}
      className={cn(`relative`, size === `inline` && `max-w-full`, className)}
    >
      {onOpen ? (
        <button
          type="button"
          data-slot="attachment-open"
          aria-label={openLabel ?? alt}
          onClick={onOpen}
          className="block cursor-zoom-in"
        >
          {media}
        </button>
      ) : (
        media
      )}
      {onRemove && (
        <AttachmentRemoveButton
          label={removeLabel ?? `Remove`}
          disabled={disabled}
          onClick={onRemove}
          className={removeClassName}
        />
      )}
    </div>
  )
}
