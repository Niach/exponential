export const PROGRESS_CLASSES = {
  root: `bg-primary/20 relative h-2 w-full overflow-hidden rounded-full`,
  indicator: `bg-primary h-full flex-1 transition-all`,
  // EXP-724: no value = INDETERMINATE (Radix's own `null` state) — a short bar
  // sweeping the track, for work with a real duration but no measurable
  // progress (the steering compaction strip). Under reduced motion it parks
  // at the left as a static, dimmed segment rather than animating.
  indicatorIndeterminate: `w-1/3 motion-safe:animate-progress-indeterminate motion-reduce:opacity-60`,
  indicatorDeterminate: `w-full`,
} as const

/** The determinate indicator's offset for `value` (0-100). */
export function progressIndicatorTransform(value: number | null | undefined): string {
  return `translateX(-${100 - (value || 0)}%)`
}
