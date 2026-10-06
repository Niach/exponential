/** A compact "5m" / "3h" / "2d" age, or the date beyond a week. */
export function relativeTime(value: string | undefined, now = Date.now()): string {
  if (!value) return ``
  const time = new Date(value).getTime()
  if (Number.isNaN(time)) return ``
  const minutes = Math.max(0, Math.round((now - time) / 60_000))
  if (minutes < 1) return `now`
  if (minutes < 60) return `${minutes}m`
  const hours = Math.round(minutes / 60)
  if (hours < 24) return `${hours}h`
  const days = Math.round(hours / 24)
  if (days < 7) return `${days}d`
  return new Date(time).toLocaleDateString(undefined, { month: `short`, day: `numeric` })
}
