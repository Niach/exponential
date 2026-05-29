import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

// Interpret a `YYYY-MM-DD` string as local midnight (plain `new Date(s)` parses
// it as UTC, which shifts the day for negative-offset timezones).
export function parseLocalDate(date: string): Date {
  return new Date(`${date}T00:00:00`)
}

export function formatDate(date: Date | string): string {
  const d = typeof date === `string` ? parseLocalDate(date) : date
  return d.toLocaleDateString(`en-US`, { month: `short`, day: `numeric` })
}

export function getInitials(value: string) {
  return value
    .split(` `)
    .map((part) => part[0] ?? ``)
    .join(``)
    .toUpperCase()
    .slice(0, 2)
}
