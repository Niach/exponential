import { avatarHueIndex } from "./avatar-color"

export const AVATAR_CLASSES = {
  root: `relative flex size-8 shrink-0 overflow-hidden rounded-full select-none`,
  image: `aspect-square size-full`,
  fallback: `flex size-full items-center justify-center rounded-full text-sm`,
  /** The fallback without a `userId`: the muted chrome. */
  fallbackMuted: `bg-muted text-muted-foreground`,
} as const

// EXP-698 r4 — a fallback given the person's `userId` paints their hue: the
// palette colour at 20% behind initials in the same colour, no stroke. Without
// an id (a repo, a bot, an unresolved reporter) it keeps the muted chrome. The
// var is read inline because Tailwind v4 cannot see a computed `bg-avatar-N`.
export function avatarFallbackHue(userId: string | null | undefined): string | undefined {
  return userId === undefined || userId === null
    ? undefined
    : `var(--avatar-${avatarHueIndex(userId)})`
}

/** The inline colours of a hued fallback. */
export function avatarFallbackColors(hue: string): { backgroundColor: string; color: string } {
  return {
    backgroundColor: `color-mix(in srgb, ${hue} 20%, transparent)`,
    color: hue,
  }
}
