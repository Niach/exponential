import * as React from "react"

import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"

// EXP-904 — the ONE pending-attachment thumbnail.
//
// The comment, launch and steer composers each drew a 64px thumb with a
// corner remove badge by copy-paste. `kind="video"` is the comment composer's
// arm (EXP-824): a muted, metadata-only `<video>` is the cheapest first-frame
// thumb. A NON-media file stays a chip at the call site, which reuses
// `AttachmentRemoveButton` for the same corner badge.
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

export function AttachmentThumb({
  src,
  alt = ``,
  kind = `image`,
  removeLabel,
  onRemove,
  disabled,
  className,
}: {
  src: string
  alt?: string
  kind?: `image` | `video`
  removeLabel: string
  onRemove: () => void
  disabled?: boolean
  className?: string
}) {
  const media = `size-16 rounded-md border border-glass-stroke-card object-cover`
  return (
    <div data-slot="attachment-thumb" className={cn(`relative`, className)}>
      {kind === `video` ? (
        <video
          src={src}
          muted
          playsInline
          preload="metadata"
          aria-label={alt}
          className={cn(media, `bg-black`)}
        />
      ) : (
        <img src={src} alt={alt} className={media} />
      )}
      <AttachmentRemoveButton
        label={removeLabel}
        disabled={disabled}
        onClick={onRemove}
      />
    </div>
  )
}
