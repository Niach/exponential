// VAPP-87: what every native painter shares — the part attributes (class +
// recipe props + forced states), prop coercions, and the label/description
// text parts the form controls repeat.

import { useMemo, type ReactNode } from "react"
import { nativeRecipeProps } from "@exponential-at/ui"
import type { UiNode } from "@exponential-at/ui"
import { useSurfaceContext } from "../context"
import { partClass, propAttribute } from "../theme-css"

export type Attrs = Record<string, unknown>

export const str = (v: unknown, fallback = ``): string => (v === undefined || v === null ? fallback : typeof v === `object` ? JSON.stringify(v) : String(v))
export const num = (v: unknown, fallback = 0): number => (typeof v === `number` && Number.isFinite(v) ? v : typeof v === `string` && v.trim() !== `` && Number.isFinite(Number(v)) ? Number(v) : fallback)
export const bool = (v: unknown): boolean => v === true || v === `true`
export const arr = <T,>(v: unknown): T[] => (Array.isArray(v) ? (v as T[]) : [])

/** `part(name, …states)` → the attributes of one part element of `node`. */
export function useParts(node: UiNode, props: Record<string, unknown>) {
  const ctx = useSurfaceContext()
  return useMemo(() => {
    const recipe = nativeRecipeProps(node.component, props, ctx.extensionDefs)
    const recipeAttrs: Attrs = {}
    for (const [k, v] of Object.entries(recipe)) recipeAttrs[propAttribute(false, k)] = String(v)
    const scope = ctx.compiled.scope
    const forcedStates = ctx.states
    return (name: string, ...states: (string | false | undefined | null)[]): Attrs => {
      const all = [...forcedStates, ...states.filter((s): s is string => Boolean(s))]
      return {
        className: `${scope} ${partClass(node.component, name)}`,
        "data-xui-part": `${node.component}/${name}`,
        ...recipeAttrs,
        ...(all.length ? { "data-xs": all.join(` `) } : {}),
      }
    }
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

/** A control's `checks` evaluated: the first failing message, or null. */
export function failingCheck(checks: unknown): string | null {
  for (const check of arr<{ condition?: unknown; message?: unknown }>(checks)) {
    if (check && typeof check === `object` && `condition` in check && !check.condition) return str(check.message, `Invalid value`)
  }
  return null
}

export function Hidden({ children }: { children: ReactNode }) {
  return <span className="xui-sr-only">{children}</span>
}
