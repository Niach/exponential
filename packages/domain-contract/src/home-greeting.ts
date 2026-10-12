// EXP-1253 (EXP-1259 D1): the Agent page's greeting — "Evening, Danny". Pure
// and byte-locked by `fixtures/composer-home.json` (`cases`), which every
// mirror replays: desktop `ui::home_greeting`, iOS
// `ExpCore/Sources/Domain/HomeGreeting.swift`, Android `domain/HomeGreeting.kt`.
// The fixture's `$comment` holds the rules; this is the reference.

import fixture from "../fixtures/composer-home.json" with { type: "json" }

export interface HomeGreetingBucket {
  id: string
  from: number
  to: number
  variants: readonly string[]
}

export const HOME_GREETING_BUCKETS: readonly HomeGreetingBucket[] = fixture.buckets
export const HOME_PLACEHOLDER: string = fixture.placeholder
export const HOME_RECENT = fixture.recent
export const HOME_GEOMETRY = fixture.geometry
export const HOME_TEST_IDS = fixture.testIds
const DAY_STARTS_AT_HOUR: number = fixture.dayStartsAtHour

/** `YYYY-MM-DDTHH:mm` of a Date in the device's local zone. */
export function localWallClock(date: Date): string {
  const pad = (value: number) => String(value).padStart(2, `0`)
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`
}

function daysFromCivil(year: number, month: number, day: number): number {
  return Math.floor(Date.UTC(year, month - 1, day) / 86_400_000)
}

function bucketFor(hour: number): HomeGreetingBucket {
  for (const bucket of HOME_GREETING_BUCKETS) {
    const inside =
      bucket.from <= bucket.to
        ? hour >= bucket.from && hour <= bucket.to
        : hour >= bucket.from || hour <= bucket.to
    if (inside) return bucket
  }
  return HOME_GREETING_BUCKETS[0]
}

/** The first whitespace-separated token of a user row's name, or ``. */
export function greetingFirstName(name: string | null | undefined): string {
  return (name ?? ``).trim().split(/\s+/)[0] ?? ``
}

/** The greeting for a local wall-clock time (`YYYY-MM-DDTHH:mm`) and a
 *  user row's name. */
export function homeGreeting(local: string, name: string | null | undefined): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(local)
  if (!match) return ``
  const [year, month, day, hour] = match.slice(1, 5).map(Number)
  const bucket = bucketFor(hour)
  // The greeting day starts at 05:00: one night keeps one variant.
  const shifted = daysFromCivil(year, month, day) * 24 + hour - DAY_STARTS_AT_HOUR
  const dayIndex = Math.floor(shifted / 24)
  const count = bucket.variants.length
  const variant = bucket.variants[((dayIndex % count) + count) % count]
  const first = greetingFirstName(name)
  return first ? variant.replace(`{name}`, first) : variant.replace(`, {name}`, ``)
}
