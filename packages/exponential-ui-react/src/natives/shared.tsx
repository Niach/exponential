// VAPP-87 + round 1: what every native painter shares — the part attributes
// (class + recipe props + forced states, plus a part-level discriminator the
// painter supplies: CodeBlock/token `kind`, Table/cell `align`, Table/row
// `striped`), prop coercions, the label/description/error parts the form
// controls repeat, and the built-in glyphs of `core.catalog.json`
// `builtinIcons` (never a hardcoded glyph for an owned part).

import { useMemo, type ReactNode } from "react"
import { coreCatalog, nativeRecipeProps } from "@exponential-at/ui"
import type { UiNode } from "@exponential-at/ui"
import { useSurfaceContext } from "../context"
import { IconGlyph } from "../icons"
import { partClass, propAttribute } from "../theme-css"

export type Attrs = Record<string, unknown>

export const str = (v: unknown, fallback = ``): string => (v === undefined || v === null ? fallback : typeof v === `object` ? JSON.stringify(v) : String(v))
export const num = (v: unknown, fallback = 0): number => (typeof v === `number` && Number.isFinite(v) ? v : typeof v === `string` && v.trim() !== `` && Number.isFinite(Number(v)) ? Number(v) : fallback)
export const bool = (v: unknown): boolean => v === true || v === `true`
export const arr = <T,>(v: unknown): T[] => (Array.isArray(v) ? (v as T[]) : [])

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
  return fmts[i].format(n)
}

export type PartFn = ((name: string, ...states: (string | false | undefined | null)[]) => Attrs) & {
  /** The same with part-level recipe props (`{kind: "keyword"}`). */
  with: (name: string, extra: Record<string, unknown>, ...states: (string | false | undefined | null)[]) => Attrs
}

/** `part(name, …states)` → the attributes of one part element of `node`. */
export function useParts(node: UiNode, props: Record<string, unknown>): PartFn {
  const ctx = useSurfaceContext()
  return useMemo(() => {
    const recipe = nativeRecipeProps(node.component, props, ctx.extensionDefs)
    const recipeAttrs: Attrs = {}
    for (const [k, v] of Object.entries(recipe)) if (typeof v !== `object`) recipeAttrs[propAttribute(false, k)] = String(v)
    const scope = ctx.compiled.scope
    const forcedStates = ctx.states
    const build = (name: string, extra: Record<string, unknown> | null, states: (string | false | undefined | null)[]): Attrs => {
      const all = [...forcedStates, ...states.filter((s): s is string => Boolean(s))]
      const own: Attrs = {}
      if (extra) for (const [k, v] of Object.entries(extra)) if (v !== undefined && v !== null) own[propAttribute(false, k)] = String(v)
      return {
        className: `${scope} ${partClass(node.component, name)}`,
        "data-xui-part": `${node.component}/${name}`,
        ...recipeAttrs,
        ...own,
        ...(all.length ? { "data-xs": all.join(` `) } : {}),
      }
    }
    const fn = ((name: string, ...states: (string | false | undefined | null)[]) => build(name, null, states)) as PartFn
    fn.with = (name, extra, ...states) => build(name, extra, states)
    return fn
  }, [node.component, props, ctx.compiled.scope, ctx.states, ctx.extensionDefs])
}

export function TextPart({ part, text, as: Tag = `span`, id, htmlFor }: { part: Attrs; text: unknown; as?: `span` | `label` | `div` | `p`; id?: string; htmlFor?: string }) {
  const s = str(text)
  if (!s) return null
  const attrs = part as Record<string, string>
  if (Tag === `label`) return <label {...attrs} id={id} htmlFor={htmlFor}>{s}</label>
  if (Tag === `div`) return <div {...attrs} id={id}>{s}</div>
  if (Tag === `p`) return <p {...attrs} id={id}>{s}</p>
  return <span {...attrs} id={id}>{s}</span>
}

/** A control's `checks` evaluated: EVERY failing message, in order. */
export function failingChecks(checks: unknown, fallback = `Invalid value`): string[] {
  const out: string[] = []
  for (const check of arr<{ condition?: unknown; message?: unknown }>(checks)) {
    if (check && typeof check === `object` && `condition` in check && (check.condition === false || check.condition === null || check.condition === undefined || check.condition === ``)) out.push(str(check.message, fallback))
  }
  return out
}

/** The first failing message, or null (kept for hosts that used it). */
export function failingCheck(checks: unknown): string | null {
  return failingChecks(checks)[0] ?? null
}

export function Hidden({ children }: { children: ReactNode }) {
  return <span className="xui-sr-only">{children}</span>
}

const BUILTIN_ICONS = coreCatalog.builtinIcons as Record<string, string>

/** The icons.json name a renderer draws for an owned part. */
export function builtinIconName(slot: string): string {
  return BUILTIN_ICONS[slot] ?? `ui-icon-placeholder`
}

/** The glyph of an owned part (`CodeBlock.copy`, `Table.sortIcon.asc`),
 *  through the host's registry first, then the renderer's fallbacks. */
export function BuiltinIcon({ slot, className, size }: { slot: string; className?: string; size?: number }) {
  const ctx = useSurfaceContext()
  return <IconGlyph icons={ctx.host.icons} name={builtinIconName(slot)} className={className} width={size} height={size} data-slot={slot} />
}
