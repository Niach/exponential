import * as React from "react"
import { Avatar as AvatarPrimitive } from "radix-ui"
import { AVATAR_CLASSES, avatarFallbackColors, avatarFallbackHue } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"

function Avatar({
  className,
  ...props
}: React.ComponentProps<typeof AvatarPrimitive.Root>) {
  return (
    <AvatarPrimitive.Root
      data-slot="avatar"
      className={cn(AVATAR_CLASSES.root, className)}
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
      className={cn(AVATAR_CLASSES.image, className)}
      {...props}
    />
  )
}

function AvatarFallback({
  className,
  userId,
  style,
  ...props
}: React.ComponentProps<typeof AvatarPrimitive.Fallback> & {
  userId?: string | null
}) {
  const hue = avatarFallbackHue(userId)
  return (
    <AvatarPrimitive.Fallback
      data-slot="avatar-fallback"
      className={cn(
        AVATAR_CLASSES.fallback,
        hue === undefined && AVATAR_CLASSES.fallbackMuted,
        className
      )}
      style={hue === undefined ? style : { ...avatarFallbackColors(hue), ...style }}
      {...props}
    />
  )
}

export { Avatar, AvatarImage, AvatarFallback }
