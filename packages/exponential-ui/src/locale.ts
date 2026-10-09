// Round 1 (docs/round-1-contract.md §4): the locale decisions every renderer
// shares (catalog/locale.json): the week start by region, the likely region
// of a bare language, deprecated language codes, the right-to-left languages
// and the directional glyphs mirrored under rtl. Formatting itself is the
// platform's ICU; these are the parts that must not differ between them.

import localeJson from "../catalog/locale.json" with { type: "json" }

export const DEFAULT_LOCALE: string = localeJson.default

export interface LocaleParts {
  language: string
  region: string | null
}

const ALIASES: Readonly<Record<string, string>> = localeJson.languageAliases

/** `pt-BR` → {pt, BR}; `zh-Hant-TW` → {zh, TW}; `de` → {de, null}. The
 *  region = the first 2-letter or 3-digit subtag after the language; a
 *  deprecated language code is normalised (`iw` → `he`, `ji` → `yi`, `in` →
 *  `id`: `languageAliases`). */
export function parseLocale(locale: string): LocaleParts {
  const tags = locale.replace(/_/g, `-`).split(`-`).filter(Boolean)
  const raw = (tags[0] ?? ``).toLowerCase()
  const language = ALIASES[raw] ?? raw
  const region = tags.slice(1).find((t) => /^[A-Za-z]{2}$|^\d{3}$/.test(t))
  return { language, region: region ? region.toUpperCase() : null }
}

const WEEK_START_BY_REGION: Record<string, number> = (() => {
  const out: Record<string, number> = {}
  for (const [day, regions] of Object.entries(localeJson.weekStart.regions)) for (const r of regions) out[r] = Number(day)
  return out
})()

/** The region a locale's data comes from: its own, else the language's
 *  likely region (CLDR likelySubtags), else null. */
export function localeRegion(locale: string): string | null {
  const { language, region } = parseLocale(locale)
  return region ?? (localeJson.likelyRegion as Record<string, string>)[language] ?? null
}

/** The first day of the week, 0 = Sunday … 6 = Saturday. */
export function weekStart(locale: string): number {
  const r = localeRegion(locale)
  return r !== null && r in WEEK_START_BY_REGION ? WEEK_START_BY_REGION[r] : localeJson.weekStart.default
}

/** `rtl` for the right-to-left languages, else `ltr`. */
export function textDirection(locale: string): `ltr` | `rtl` {
  return localeJson.rtl.includes(parseLocale(locale).language) ? `rtl` : `ltr`
}

/** The directional glyphs (icons.json names) a painter mirrors under rtl. */
export const RTL_MIRRORED_ICONS: readonly string[] = localeJson.rtlMirroredIcons

/** True when `icon` is drawn mirrored (scaleX(-1), outermost) in this
 *  direction. */
export function mirrorsInRtl(icon: string, direction: `ltr` | `rtl`): boolean {
  return direction === `rtl` && RTL_MIRRORED_ICONS.includes(icon)
}
