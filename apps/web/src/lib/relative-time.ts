// The LIST caption's relative time ×4 (inbox rows, drafts, device "Last
// seen", run rows): compact `just now · 5m · 2h · 3d`, the twin of desktop's
// `inbox::relative_time`. Activity and comments keep the long form
// (`comment-rows/format.ts` `relativeTime`, "5 minutes ago").
export function compactRelativeTime(
  value: Date | string,
  now: number = Date.now()
): string {
  const d = value instanceof Date ? value : new Date(value)
  if (Number.isNaN(d.getTime())) return ``
  const mins = Math.round((now - d.getTime()) / 60000)
  if (mins < 1) return `just now`
  if (mins < 60) return `${mins}m`
  const hrs = Math.round(mins / 60)
  if (hrs < 24) return `${hrs}h`
  return `${Math.round(hrs / 24)}d`
}
