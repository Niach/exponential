// VAPP-87 + round 1 (FEED-128: hoisted from the React renderer's
// natives/shared.tsx): the prop coercions every native painter applies, the
// built-in phrase lookup, localized file sizes, the `checks` evaluation and
// the built-in glyph names of `core.catalog.json` `builtinIcons`. Pure:
// every web renderer paints with the same values.

import { DEFAULT_STRINGS, coreCatalog } from "@exponential_app/eui"

export const str = (v: unknown, fallback = ``): string => (v === undefined || v === null ? fallback : typeof v === `object` ? JSON.stringify(v) : String(v))
export const num = (v: unknown, fallback = 0): number => (typeof v === `number` && Number.isFinite(v) ? v : typeof v === `string` && v.trim() !== `` && Number.isFinite(Number(v)) ? Number(v) : fallback)
export const bool = (v: unknown): boolean => v === true || v === `true`
export const arr = <T>(v: unknown): T[] => (Array.isArray(v) ? (v as T[]) : [])
/** An array of OBJECTS (options, items, tabs, columns, rows…): entries that
 *  are not objects (`options: [null]`) are skipped, never painted. */
export const objects = <T>(v: unknown): T[] => (Array.isArray(v) ? (v.filter((e) => e !== null && typeof e === `object` && !Array.isArray(e)) as T[]) : [])

/** A built-in PHRASE with placeholders (`removeItem` = `Remove {name}`):
 *  the surface's string table entry when it carries `id` (word order is the
 *  translation's), else `fallback()`, the round-1 composition kept until
 *  catalog/strings.json ships the id. */
export function phrase(ctx: { strings: Readonly<Record<string, string>>; t: (id: string, params?: Record<string, unknown>) => string }, id: string, params: Record<string, unknown>, fallback: () => string): string {
  return ctx.strings[id] !== undefined ? ctx.t(id, params) : fallback()
}

const SIZE_UNITS = [`byte`, `kilobyte`, `megabyte`, `gigabyte`] as const
const sizeFormats = new Map<string, Intl.NumberFormat[]>()

/** A byte count in the surface locale with a LOCALIZED unit (1024 steps,
 *  `Intl.NumberFormat` `style: "unit"`, e.g. `1,5 kB` / `١٫٥ ك.بايت`). */
export function formatFileSize(bytes: number, locale: string): string {
  let fmts = sizeFormats.get(locale)
  if (!fmts) {
    fmts = SIZE_UNITS.map((unit) => {
      try {
        return new Intl.NumberFormat(locale, { style: `unit`, unit, unitDisplay: `short`, maximumFractionDigits: 1 })
      } catch {
        return new Intl.NumberFormat(locale, { maximumFractionDigits: 1 })
      }
    })
    sizeFormats.set(locale, fmts)
  }
  let i = 0
  let n = Math.max(0, bytes)
  while (n >= 1024 && i < SIZE_UNITS.length - 1) {
    n /= 1024
    i++
  }
  return fmts[i]!.format(n)
}

/** Does a file match an `accept` list (`image/*,.pdf`)? */
export function acceptsFile(accept: string, file: { name: string; type: string }): boolean {
  const rules = accept
    .split(`,`)
    .map((r) => r.trim().toLowerCase())
    .filter(Boolean)
  if (rules.length === 0) return true
  const name = file.name.toLowerCase()
  const type = (file.type || ``).toLowerCase()
  return rules.some((r) => (r.startsWith(`.`) ? name.endsWith(r) : r.endsWith(`/*`) ? type.startsWith(r.slice(0, -1)) : type === r))
}

/** A control's `checks` evaluated: EVERY failing message, in order (a
 *  check without one says `$string.invalidValue`: pass the surface's). */
export function failingChecks(checks: unknown, fallback: string = DEFAULT_STRINGS.invalidValue): string[] {
  const out: string[] = []
  for (const check of objects<{ condition?: unknown; message?: unknown }>(checks)) {
    if (check && typeof check === `object` && `condition` in check && (check.condition === false || check.condition === null || check.condition === undefined || check.condition === ``)) out.push(str(check.message, fallback))
  }
  return out
}

/** The first failing message, or null (kept for hosts that used it). */
export function failingCheck(checks: unknown, fallback?: string): string | null {
  return failingChecks(checks, fallback)[0] ?? null
}

const BUILTIN_ICONS = coreCatalog.builtinIcons as Record<string, string>

/** The icons.json name a renderer draws for an owned part. */
export function builtinIconName(slot: string): string {
  return BUILTIN_ICONS[slot] ?? `ui-icon-placeholder`
}

/** Space-joined element ids (`aria-describedby`), undefined when none. */
export function joinIds(...ids: (string | undefined | false)[]): string | undefined {
  const out = ids.filter(Boolean).join(` `)
  return out || undefined
}
