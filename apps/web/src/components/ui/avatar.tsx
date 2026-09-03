import * as React from "react"
import { Avatar as AvatarPrimitive } from "radix-ui"

import { avatarHueIndex } from "@/lib/avatar-color"
import { cn } from "@/lib/utils"

function Avatar({
  className,
  ...props
}: React.ComponentProps<typeof AvatarPrimitive.Root>) {
  return (
    <AvatarPrimitive.Root
      data-slot="avatar"
      className={cn(
        `relative flex size-8 shrink-0 overflow-hidden rounded-full select-none`,
        className
      )}
      {...props}
    />
  )
}

function AvatarImage({
  className,
  ...props
}: React.ComponentProps<typeof AvatarPrimitive.Image>) {
  return (
    <AvatarPrimitive.Image
      data-slot="avatar-image"
      className={cn(`aspect-square size-full`, className)}
      {...props}
    />
  )
}

// EXP-698 r4 — a fallback given the person's `userId` paints their hue: the
// palette colour at 20% behind initials in the same colour, no stroke. Without
// an id (a repo, a bot, an unresolved reporter) it keeps the muted chrome. The
// var is read inline because Tailwind v4 cannot see a computed `bg-avatar-N`.
function AvatarFallback({
  className,
  userId,
  style,
  ...props
}: React.ComponentProps<typeof AvatarPrimitive.Fallback> & {
  userId?: string | null
}) {
  const hue =
    userId === undefined || userId === null
      ? undefined
      : `var(--avatar-${avatarHueIndex(userId)})`
  return (
    <AvatarPrimitive.Fallback
      data-slot="avatar-fallback"
      className={cn(
        `flex size-full items-center justify-center rounded-full text-sm`,
        hue === undefined && `bg-muted text-muted-foreground`,
        className
      )}
      style={
        hue === undefined
          ? style
          : {
              backgroundColor: `color-mix(in srgb, ${hue} 20%, transparent)`,
              color: hue,
              ...style,
            }
      }
      {...props}
    />
  )
}

export { Avatar, AvatarImage, AvatarFallback }
