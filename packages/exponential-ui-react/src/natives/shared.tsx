// VAPP-87 + round 1: what every native painter shares — the part attributes
// (class + recipe props + forced states, plus a part-level discriminator the
// painter supplies: CodeBlock/token `kind`, Table/cell `align`, Table/row
// `striped`), prop coercions, the label/description/error parts the form
// controls repeat, and the built-in glyphs of `core.catalog.json`
// `builtinIcons` (never a hardcoded glyph for an owned part).

import { useContext, useMemo, type ReactNode } from "react"
import { DEFAULT_STRINGS, coreCatalog, displayString, nativeRecipeProps } from "@exponential-at/ui"
import type { UiNode } from "@exponential-at/ui"
import { InstanceContext, useSurfaceContext } from "../context"
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
  /** A PER-ITEM placed part (round 2 §6): `data-xui-id="<id>.<part>.<item>"`
   *  (`item` = an index or a row key: `tab.1`, `cell.alice.0`). */
  at: (name: string, item: string | number, ...states: (string | false | undefined | null)[]) => Attrs
  /** `at` with part-level recipe props. */
  atWith: (name: string, item: string | number, extra: Record<string, unknown>, ...states: (string | false | undefined | null)[]) => Attrs
  /** The painted id of the node (`<id><instance suffix>`). */
  domId: string
}

/** Round 2 §6 (catalog/recipes.json): the SINGLE parts a painter lays out
 *  and the conformance dump compares, by component. A placed part carries
 *  `data-xui-id="<node id>.<part>"`; per-item parts go through `part.at`.
 *  Paint-only parts (thumbs, the Slider range, the check glyph, arrows,
 *  overlay content in its layer) carry no id. */
const PLACED: Readonly<Record<string, readonly string[]>> = {
  Switch: [`body`, `label`, `description`, `track`],
  Input: [`label`, `field`, `description`, `error`],
  Textarea: [`label`, `field`, `description`, `error`],
  NumberField: [`label`, `field`, `decrement`, `input`, `unit`, `increment`, `description`, `error`],
  ChipInput: [`label`, `field`, `input`, `description`, `error`],
  Select: [`label`, `trigger`],
  DatePicker: [`label`, `trigger`],
  DateRangePicker: [`label`, `trigger`],
  TimePicker: [`label`, `trigger`],
  Radio: [`label`, `items`],
  Checkbox: [`box`, `label`, `description`],
  Slider: [`header`, `label`, `value`, `track`],
  Tabs: [`list`, `content`],
  Table: [`header`, `body`, `caption`, `empty`],
  Carousel: [`page`, `indicator`, `controls`, `previous`, `next`],
  Tooltip: [`anchor`],
  DropdownMenu: [`trigger`],
  FileUpload: [`label`, `dropzone`, `icon`, `title`, `hint`, `browse`, `description`],
  CodeBlock: [`header`, `title`, `copy`, `body`],
  AudioPlayer: [`track`, `controls`],
  Video: [`controls`],
  Image: [`fallback`],
  Ring: [`label`],
}

/** `part(name, …states)` → the attributes of one part element of `node`. */
export function useParts(node: UiNode, props: Record<string, unknown>): PartFn {
  const ctx = useSurfaceContext()
  const instance = useContext(InstanceContext)
  return useMemo(() => {
    const recipe = nativeRecipeProps(node.component, props, ctx.extensionDefs)
    const recipeAttrs: Attrs = {}
    for (const [k, v] of Object.entries(recipe)) if (typeof v !== `object`) recipeAttrs[propAttribute(false, k)] = String(v)
    const scope = ctx.compiled.scope
    const forcedStates = ctx.states
    const domId = `${node.id}${instance}`
    const placed = PLACED[node.component]
    const build = (name: string, extra: Record<string, unknown> | null, states: (string | false | undefined | null)[], id: string | null): Attrs => {
      const all = [...forcedStates, ...states.filter((s): s is string => Boolean(s))]
      const own: Attrs = {}
      if (extra) for (const [k, v] of Object.entries(extra)) if (v !== undefined && v !== null) own[propAttribute(false, k)] = String(v)
      return {
        className: `${scope} ${partClass(node.component, name)}`,
        "data-xui-part": `${node.component}/${name}`,
        ...(id !== null ? { "data-xui-id": id } : {}),
        ...recipeAttrs,
        ...own,
        ...(all.length ? { "data-xs": all.join(` `) } : {}),
      }
    }
    const single = (name: string) => (placed?.includes(name) ? `${domId}.${name}` : null)
    const fn = ((name: string, ...states: (string | false | undefined | null)[]) => build(name, null, states, single(name))) as PartFn
    fn.with = (name, extra, ...states) => build(name, extra, states, single(name))
    fn.at = (name, item, ...states) => build(name, null, states, `${domId}.${name}.${item}`)
    fn.atWith = (name, item, extra, ...states) => build(name, extra, states, `${domId}.${name}.${item}`)
    fn.domId = domId
    return fn
  }, [node.component, node.id, instance, props, ctx.compiled.scope, ctx.states, ctx.extensionDefs])
}

export function TextPart({ part, text, as: Tag = `span`, id, htmlFor }: { part: Attrs; text: unknown; as?: `span` | `label` | `div` | `p`; id?: string; htmlFor?: string }) {
  // Round 2 §3: a bound number shows `412`, a boolean `true`; objects nothing.
  const s = displayString(text)
  if (!s) return null
  const attrs = part as Record<string, string>
  if (Tag === `label`) return <label {...attrs} id={id} htmlFor={htmlFor}>{s}</label>
  if (Tag === `div`) return <div {...attrs} id={id}>{s}</div>
  if (Tag === `p`) return <p {...attrs} id={id}>{s}</p>
  return <span {...attrs} id={id}>{s}</span>
}

/** A control's `checks` evaluated: EVERY failing message, in order (a
 *  check without one says `$string.invalidValue`: pass the surface's). */
export function failingChecks(checks: unknown, fallback: string = DEFAULT_STRINGS.invalidValue): string[] {
  const out: string[] = []
  for (const check of arr<{ condition?: unknown; message?: unknown }>(checks)) {
    if (check && typeof check === `object` && `condition` in check && (check.condition === false || check.condition === null || check.condition === undefined || check.condition === ``)) out.push(str(check.message, fallback))
  }
  return out
}

/** The first failing message, or null (kept for hosts that used it). */
export function failingCheck(checks: unknown, fallback?: string): string | null {
  return failingChecks(checks, fallback)[0] ?? null
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
