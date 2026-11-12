// #FEED-232: THE count badge's label (the CountBadge native, the web
// `Badge` primitive, the gpui `count_badge`; domain-contract nav-badges.json
// `labelCases`). Zero or less (or not a number) = `` = NOTHING renders;
// past `max` it reads `${max}+`. The Rust core's `count_badge` ports it.

export const COUNT_BADGE_MAX = 99

export const COUNT_BADGE_TONES = [`attention`, `muted`] as const
export type CountBadgeTone = (typeof COUNT_BADGE_TONES)[number]

/** What the badge prints for `count`; `` = no badge. */
export function countBadgeLabel(count: unknown, max: unknown = COUNT_BADGE_MAX): string {
  const n = typeof count === `number` && Number.isFinite(count) ? Math.floor(count) : 0
  if (n <= 0) return ``
  const cap = typeof max === `number` && Number.isFinite(max) && max >= 1 ? Math.floor(max) : COUNT_BADGE_MAX
  return n > cap ? `${cap}+` : String(n)
}

/** The tone a prop names (`muted` for anything else). */
export function countBadgeTone(tone: unknown): CountBadgeTone {
  return tone === `attention` ? `attention` : `muted`
}
