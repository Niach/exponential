// FEED-128: what every native painter shares on Solid (React's
// natives/shared.tsx): the part attributes, prop coercions, the text part
// the form controls repeat, and the built-in glyphs of `core.catalog.json`
// `builtinIcons`.
//
// ── THE RULES FOR PORTING A NATIVE (the fan-out agents) ──────────────────
// 1. The React file is the SPEC. The Solid painter produces the IDENTICAL
//    DOM: same elements, same attribute set and values (`class` strings in
//    the same order, `data-xui-id`, `data-xui-part`, `data-xui-c`,
//    `data-r-*`/`data-m-*`, `data-xs`, aria-*, role, tabindex), same text.
//    The web-parity suite diffs the serialised DOM, every computed style,
//    frames and pixels at zero tolerance. Extend `src/test/parity.ts`
//    (`expectDomParity`) with a case per native you port.
// 2. Attribute NAMES: React's `className` is `class`, `htmlFor` is `for`,
//    `tabIndex` is `tabindex`; React renders `true`/`false` for `aria-*`
//    and `data-*` as the strings "true"/"false" (so does Solid), omits
//    `undefined`/`null`. Inline styles go through `css()` (React's
//    semantics: camelCase → kebab, numbers → px unless unitless) or
//    `mergeStyle(rootProps, reactStyleObject)`. React writes a controlled
//    `<input checked>` / `value` / `defaultValue` as the ATTRIBUTE too
//    (`checked=""`), Solid only sets the property: add
//    `attr:checked={on ? `` : undefined}` (markdown.tsx does).
//    NEVER write an object literal with COMPUTED keys straight into
//    `style={{ [side]: … }}`: the Solid compiler loses them (seen in the
//    harness); build the object first or pass it through `css()`.
// 3. STATE ATTRIBUTES: the theme CSS (`STATE_SELECTORS`, web/theme-css.ts)
//    targets the RADIX names. Kobalte writes its own (`data-checked`,
//    `data-expanded`, `data-closed`, `data-highlighted` on different nodes,
//    `data-selected`…), Radix writes `data-state="checked|unchecked|open|
//    closed|active|inactive|on|off"`, `data-side`, `data-align`,
//    `data-highlighted`, `data-disabled`, `data-orientation`. The Solid
//    painter sets the RADIX-named attributes ITSELF on every part React's
//    Radix part carries them (`data-state`, `data-side`, `data-align`,
//    `data-highlighted`, `data-disabled`, `data-orientation`, with Radix's
//    values: `data-disabled=""` when disabled, absent otherwise), and keeps a
//    Kobalte-generated attribute ONLY where React's Radix DOM carries the
//    same attribute with the same value (strip or override the rest: render
//    Kobalte's part `as` your own element and spread only the attributes
//    you need, or use the headless primitives). Generated ids (`kb-…`,
//    `radix-…`, `:r0:`) are normalised by the parity helper, the
//    attributes that REFERENCE them (`aria-controls`, `aria-labelledby`,
//    `aria-describedby`, `for`) must still exist on the same elements.
// 4. Reactivity: `NativeProps` fields are live getters, never destructure
//    them; read `props.children` and each slot ONCE; derive with
//    `createMemo`, never mirror props into signals except for the bindable
//    state rule (`createBoundState`).
// 5. A painter that is not ported yet stays `Pending` (natives/pending.tsx).

import { createMemo, type JSX } from "solid-js"
import { displayString, nativeRecipeProps } from "@exponential_app/eui"
import { PLACED, arr, bool, builtinIconName, failingCheck, failingChecks, formatFileSize, num, objects, partClass, phrase, propAttribute, str } from "@exponential_app/eui/web"
import type { UiNode } from "@exponential_app/eui"
import { useInstance, useSurfaceContext } from "../context"
import { IconGlyph } from "../icons"

export type Attrs = Record<string, unknown>

export { str, num, bool, arr, objects, phrase, formatFileSize, failingChecks, failingCheck, builtinIconName }
export { css, px } from "../style"
export type { StyleInput, SolidStyle } from "../style"

type State = string | false | undefined | null

export type PartFn = ((name: string, ...states: State[]) => Attrs) & {
  /** The same with part-level recipe props (`{kind: "keyword"}`). */
  with: (name: string, extra: Record<string, unknown>, ...states: State[]) => Attrs
  /** A PER-ITEM placed part: `data-xui-id="<id>.<part>.<item>"`. */
  at: (name: string, item: string | number, ...states: State[]) => Attrs
  /** `at` with part-level recipe props. */
  atWith: (name: string, item: string | number, extra: Record<string, unknown>, ...states: State[]) => Attrs
  /** The painted id of the node (`<id><instance suffix>`), live. */
  readonly domId: string
}

/** React's `useParts(node, props)` on Solid: `parts(props)` takes the
 *  painter's props object (`{node, props}` live getters) and returns
 *  `part(name, …states)` → the attribute bag of one part element, keys in
 *  React's order: `class` (`<scope> xui-<Component>-<part>`),
 *  `data-xui-part` (`<Component>/<part>`), `data-xui-id` (placed parts
 *  only), the native recipe props (`data-r-*`), the part's own extras,
 *  `data-xs`. Call `part(…)` INSIDE JSX (or a memo) so it follows the
 *  node, its props, the theme scope and the forced states. */
export function parts(source: { readonly node: UiNode; readonly props: Record<string, unknown> }): PartFn {
  const ctx = useSurfaceContext()
  const instance = useInstance()
  const recipeAttrs = createMemo(() => {
    const out: Attrs = {}
    for (const [k, v] of Object.entries(nativeRecipeProps(source.node.component, source.props, ctx.extensionDefs))) if (typeof v !== `object`) out[propAttribute(false, k)] = String(v)
    return out
  })
  const domId = () => `${source.node.id}${instance()}`
  const build = (name: string, extra: Record<string, unknown> | null, states: State[], item: string | number | null): Attrs => {
    const component = source.node.component
    const id = item !== null ? `${domId()}.${name}.${item}` : PLACED[component]?.includes(name) ? `${domId()}.${name}` : null
    const all = [...ctx.states, ...states.filter((s): s is string => Boolean(s))]
    const own: Attrs = {}
    if (extra) for (const [k, v] of Object.entries(extra)) if (v !== undefined && v !== null) own[propAttribute(false, k)] = String(v)
    return {
      class: `${ctx.compiled.scope} ${partClass(component, name)}`,
      "data-xui-part": `${component}/${name}`,
      ...(id !== null ? { "data-xui-id": id } : {}),
      ...recipeAttrs(),
      ...own,
      ...(all.length ? { "data-xs": all.join(` `) } : {}),
    }
  }
  const fn = ((name: string, ...states: State[]) => build(name, null, states, null)) as PartFn
  fn.with = (name, extra, ...states) => build(name, extra, states, null)
  fn.at = (name, item, ...states) => build(name, null, states, item)
  fn.atWith = (name, item, extra, ...states) => build(name, extra, states, item)
  Object.defineProperty(fn, `domId`, { get: domId })
  return fn
}

/** React's name, for a literal port. */
export const useParts = (node: UiNode, props: Record<string, unknown>): PartFn => parts({ node, props })

/** A text part (React's `TextPart`): nothing when the text displays empty. */
export function TextPart(props: { part: Attrs; text: unknown; as?: `span` | `label` | `div` | `p`; id?: string; htmlFor?: string }): JSX.Element {
  const s = () => displayString(props.text)
  return (
    <>
      {s()
        ? (() => {
            const Tag = props.as ?? `span`
            if (Tag === `label`) return <label {...props.part} id={props.id} for={props.htmlFor}>{s()}</label>
            if (Tag === `div`) return <div {...props.part} id={props.id}>{s()}</div>
            if (Tag === `p`) return <p {...props.part} id={props.id}>{s()}</p>
            return <span {...props.part} id={props.id}>{s()}</span>
          })()
        : null}
    </>
  )
}

export function Hidden(props: { children: JSX.Element }): JSX.Element {
  return <span class="xui-sr-only">{props.children}</span>
}

/** The glyph of an owned part (`CodeBlock.copy`, `Table.sortIcon.asc`),
 *  through the host's registry first, then the renderer's fallbacks. */
export function BuiltinIcon(props: { slot: string; class?: string; size?: number }): JSX.Element {
  const ctx = useSurfaceContext()
  return <IconGlyph icons={ctx.host.icons} name={builtinIconName(props.slot)} class={props.class} width={props.size} height={props.size} data-slot={props.slot} />
}
