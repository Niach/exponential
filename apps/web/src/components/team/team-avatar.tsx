import { cn } from "@/lib/utils"

// EXP-698 r5 — the team mark, one component instead of a hand-rolled square in
// every switcher: the PRIMARY fill under its foreground, a rounded square at a
// quarter of its side (iOS `TeamAvatar.swift`, Android `Avatars.kt`), and the
// team's first letter. User avatars stay hashed-hue circles; a team is a
// single accent square, so the two never read as the same thing.
export function TeamAvatar({
  name,
  size = 20,
  className,
}: {
  name: string | undefined | null
  /** Side in px. 28 in the sidebar header, 20 in its menu, 18 in the sheet. */
  size?: number
  className?: string
}) {
  const initial = (name ?? ``).trim()[0]?.toUpperCase() ?? `E`
  return (
    <span
      aria-hidden
      className={cn(
        `flex shrink-0 items-center justify-center bg-primary font-bold text-primary-foreground`,
        className
      )}
      style={{
        width: size,
        height: size,
        borderRadius: size / 4,
        fontSize: Math.max(9, Math.round(size * 0.44)),
      }}
    >
      {initial}
    </span>
  )
}
