// EXP-887: `cn` and `getInitials` moved into @exp/ui (every component in the
// package styles through them, and a package cannot import the app). They are
// re-exported here so the ~200 `@/lib/utils` call sites keep working and there
// is still ONE `cn` in the process.
export { cn, getInitials } from "@exp/ui"

// Interpret a `YYYY-MM-DD` string as local midnight (plain `new Date(s)` parses
// it as UTC, which shifts the day for negative-offset timezones).
export function parseLocalDate(date: string): Date {
  return new Date(`${date}T00:00:00`)
}

export function formatDate(date: Date | string): string {
  const d = typeof date === `string` ? parseLocalDate(date) : date
  return d.toLocaleDateString(`en-US`, { month: `short`, day: `numeric` })
}
