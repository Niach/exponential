import { Avatar, AvatarFallback, AvatarImage } from "./avatar"
import { cn, getInitials } from "./cn"

// EXP-887 — the ONE user mark. The Avatar + AvatarImage + AvatarFallback +
// getInitials composition was hand-rolled at every site that draws a person;
// this is that composition, once.
//
// Name-less accounts (Apple sign-in) fall back to the EMAIL for initials
// instead of a bare "?", and the fallback carries the user id so it paints
// that person's hashed hue (EXP-698 r4, `avatar-color.ts`).

/** Side in px → the root's size class and the fallback's type scale. The
 *  table is explicit because Tailwind cannot see a computed `size-<n>`. */
export const USER_AVATAR_SIZES = {
  16: { root: `size-4`, fallback: `text-[0.5rem]` },
  20: { root: `size-5`, fallback: `text-[0.5625rem]` },
  24: { root: `h-6 w-6`, fallback: `text-xs` },
  28: { root: `size-7`, fallback: `text-xs` },
  32: { root: `h-8 w-8`, fallback: `text-xs` },
  48: { root: `h-12 w-12`, fallback: `text-sm` },
} as const

export type UserAvatarSize = keyof typeof USER_AVATAR_SIZES

export interface UserAvatarUser {
  id?: string | null
  name?: string | null
  email?: string | null
  image?: string | null
}

export function UserAvatar({
  user,
  size = 24,
  className,
}: {
  user: UserAvatarUser | null | undefined
  size?: UserAvatarSize
  className?: string
}) {
  const label = user?.name || user?.email
  const scale = USER_AVATAR_SIZES[size]
  return (
    <Avatar className={cn(scale.root, className)}>
      {/* The person's name is spelled out beside the mark nearly everywhere,
          so the photo carries the name only when there IS one to carry. */}
      {user?.image && <AvatarImage src={user.image} alt={label ?? ``} />}
      <AvatarFallback className={scale.fallback} userId={user?.id}>
        {label ? getInitials(label) : `?`}
      </AvatarFallback>
    </Avatar>
  )
}
