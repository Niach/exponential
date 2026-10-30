// VAPP-87 + round 1: a surface's NODE stylesheet (layer `xui-node`). Per
// node, in this order (contract §2, `resolveConditions` is the reference):
//   1. its whitelisted `style` (tokens as `var(--xui-…)`);
//   2. every `@media` block in SOURCE order (later wins): width conditions
//      as CONTAINER queries on the surface's `xui` container (`min-width` =
//      `width >= N`, `max-width` = `width < N`, `$breakpoint.*` resolved from
//      the theme), height and orientation as attribute-gated rules the
//      surface sets from its measured box (`data-xq`, a container query
//      cannot ask an inline-size container for its height), `hover` and
//      `prefers-reduced-motion` as real media queries;
//   3. the state blocks `:hover`, `:focus-visible`, `:pressed`.
// A style value the macro expander left DYNAMIC (a binding or a call, e.g.
// Progress `width: percent{…}`) becomes `var(--xd-<n>)`; `NodeView` sets
// the resolved value on the element, so the layer order (native recipe <
// node style < macro part < authored) still holds. VAPP-163: the keys a
// macro node's author wrote (`recipe.authored`) are emitted AGAIN, the same
// way, in the `xui-author` layer, which sits after `xui-part`. Template subtrees are included
// (their items render under the template node's own class), so a templated
// row wears its author and macro styles (audit bug B).

import { parseMediaCondition, parseTokenRef, type FontSpec, type ResolvedTheme, type UiNode } from "@exponential_app/eui"
import { hasDynamic } from "./data"
import { declarations, rule, shimmerRule } from "./theme-css"

export const nodeClass = (id: string): string => `xui-n-${cssEscape(id)}`
export const surfaceClass = (surfaceId: string): string => `xui-s-${cssEscape(surfaceId)}`

/** Ids are author strings; keep the class usable in a selector. */
export function cssEscape(text: string): string {
  return text.replace(/[^a-zA-Z0-9_-]/g, (c) => `_${c.charCodeAt(0).toString(16)}_`)
}

export function* walkNodes(root: UiNode): Generator<UiNode> {
  yield root
  for (const slot of Object.values(root.slots ?? {})) yield* walkNodes(slot)
  for (const child of root.children) yield* walkNodes(child)
}

/** One dynamic style value of a node: where it sits and its variable. */
export interface DynamicStyleEntry {
  /** `--xd-<n>`, numbered in walk order (base keys, then each block). */
  name: string
  /** The condition key (`@media …`, `:hover`) or `` for the base style. */
  block: string
  key: string
  value: unknown
}

/** The node's dynamic style values in the order the sheet numbers them. */
export function dynamicStyleEntries(style: Record<string, unknown> | undefined): DynamicStyleEntry[] {
  const out: DynamicStyleEntry[] = []
  if (!style) return out
  const visit = (block: string, s: Record<string, unknown>) => {
    for (const [key, value] of Object.entries(s)) {
      if (key.startsWith(`@`) || key.startsWith(`:`)) continue
      if (hasDynamic(value)) out.push({ name: `--xd-${out.length}`, block, key, value })
    }
  }
  visit(``, style)
  for (const [key, value] of Object.entries(style)) {
    if ((key.startsWith(`@`) || key.startsWith(`:`)) && typeof value === `object` && value !== null) visit(key, value as Record<string, unknown>)
  }
  return out
}

/** A JS-evaluated condition (height / orientation): its `data-xq` token. */
export function queryToken(key: string): string {
  return cssEscape(key.replace(/^@media \(|\)$/g, ``).replace(/: /, `-`))
}

export interface NodeSheet {
  css: string
  /** The resolved condition keys the surface must evaluate itself and list
   *  in `data-xq` when they hold (height, orientation). */
  queries: string[]
  /** VAPP-152: each styled node's rules by id, in walk order (`css` is
   *  their concatenation inside the `xui-node` layer). VAPP-163: a node
   *  with authored keys appends `AUTHOR_SEP` + its `xui-author` rules;
   *  `layeredCss` splits them into the two layers. */
  chunks: ReadonlyMap<string, string>
}

/** The key a surface evaluates itself for a `@media` block (size values
 *  resolved to px: `@media (min-width: 768px)`); null = never matches. */
export function resolvedMediaKey(key: string, theme: ResolvedTheme | undefined): string | null {
  const condition = parseMediaCondition(key)
  if (!condition) return null
  if (condition.feature === `orientation` || condition.feature === `hover` || condition.feature === `prefers-reduced-motion`) return key
  const px = breakpointPx(condition.value, theme)
  return px === null ? null : `@media (${condition.feature}: ${px}px)`
}

const isDirection = (v: unknown): v is `ltr` | `rtl` => v === `ltr` || v === `rtl`

/** A node's own `direction` (round 2 §2): its base style's, then every
 *  MATCHING `@media` block's in source order (later wins), as
 *  `resolveConditions` flattens it; `matches` = the surface's `data-xq`. */
export function styleDirection(style: Record<string, unknown> | undefined, theme: ResolvedTheme | undefined, matches: (resolvedKey: string) => boolean): `ltr` | `rtl` | undefined {
  if (!style) return undefined
  let out = isDirection(style.direction) ? style.direction : undefined
  for (const [key, value] of Object.entries(style)) {
    if (!key.startsWith(`@`) || typeof value !== `object` || value === null) continue
    const d = (value as Record<string, unknown>).direction
    if (!isDirection(d)) continue
    const resolved = resolvedMediaKey(key, theme)
    if (resolved && matches(resolved)) out = d
  }
  return out
}

function breakpointPx(value: string, theme: ResolvedTheme | undefined): number | null {
  const ref = parseTokenRef(value)
  if (ref) {
    if (ref.group !== `breakpoint`) return null
    const px = theme?.tokens.breakpoint?.[ref.name]
    return typeof px === `number` ? px : null
  }
  const n = parseFloat(value)
  return Number.isFinite(n) ? n : null
}

/** VAPP-163: separates a chunk's `xui-node` rules from its `xui-author`
 *  rules (NUL never survives in CSS, so it never occurs in a rule). */
export const AUTHOR_SEP = `\u0000`

/** Chunks (`NodeSheet.chunks`) as one stylesheet: every node's rules in the
 *  `xui-node` layer, then the authored ones in `xui-author`. */
export function layeredCss(chunks: Iterable<string>): string {
  const node: string[] = []
  const author: string[] = []
  for (const chunk of chunks) {
    const at = chunk.indexOf(AUTHOR_SEP)
    if (at < 0) node.push(chunk)
    else {
      node.push(chunk.slice(0, at))
      author.push(chunk.slice(at + 1))
    }
  }
  return `@layer xui-node{${node.join(``)}}${author.length ? `@layer xui-author{${author.join(``)}}` : ``}`
}

interface NodeChunk {
  css: string
  queries: readonly string[]
}

/** VAPP-152: a node's rules, per node OBJECT (a re-reduced tree shares its
 *  unchanged nodes) and the inputs they depend on. */
const chunkCache = new WeakMap<UiNode, { scope: string; theme: ResolvedTheme | undefined; fonts: Record<string, FontSpec>; chunk: NodeChunk }>()

function nodeChunk(node: UiNode, style: Record<string, unknown>, scope: string, theme: ResolvedTheme | undefined, fonts: Record<string, FontSpec>): NodeChunk {
  const hit = chunkCache.get(node)
  if (hit && hit.scope === scope && hit.theme === theme && hit.fonts === fonts) return hit.chunk
  const queries = new Set<string>()
  // The variables are numbered over the WHOLE style, so the authored rules
  // read the same `--xd-<n>` the element carries.
  const dyn = dynamicStyleEntries(style)
  const withVars = (block: string, s: Record<string, unknown>) => {
    const mine = dyn.filter((d) => d.block === block && d.key in s)
    if (mine.length === 0) return s
    const copy = { ...s }
    for (const d of mine) copy[d.key] = `var(${d.name})`
    return copy
  }
  const cls = nodeClass(node.id)
  const own = withVars(``, style)
  let css = styleRules(style, own, withVars, scope, cls, theme, fonts, queries)
  const authored = node.recipe?.authored
  if (authored?.length) {
    const picked: Record<string, unknown> = {}
    for (const key of authored) if (key in style) picked[key] = style[key]
    const author = styleRules(picked, own, withVars, scope, cls, theme, fonts, queries)
    if (author) css += AUTHOR_SEP + author
  }
  const chunk = { css, queries: [...queries] }
  chunkCache.set(node, { scope, theme, fonts, chunk })
  return chunk
}

/** The rules of one style object (a node's whole style, or its authored
 *  keys): base declarations, `@media` blocks, state blocks. `own` = the
 *  node's whole base style (the context `declarations` reads). */
function styleRules(
  style: Record<string, unknown>,
  own: Record<string, unknown>,
  withVars: (block: string, s: Record<string, unknown>) => Record<string, unknown>,
  scope: string,
  cls: string,
  theme: ResolvedTheme | undefined,
  fonts: Record<string, FontSpec>,
  queries: Set<string>
): string {
  const sel = `${scope} .${cls}`
  const rules: string[] = []
  const base = declarations(withVars(``, style), fonts, own)
  if (base.length) rules.push(rule(sel, base) + shimmerRule(sel, style))
  for (const [key, value] of Object.entries(style)) {
    if (!key.startsWith(`@`) || typeof value !== `object` || value === null) continue
    const block = declarations(withVars(key, value as Record<string, unknown>), fonts, own)
    if (block.length === 0) continue
    const condition = parseMediaCondition(key)
    if (!condition) continue
    // A block that sets `direction` is ALSO evaluated by the surface
    // (`data-xq`): the node's direction drives `dir`, Radix and keys.
    if (isDirection((value as Record<string, unknown>).direction)) {
      const resolved = resolvedMediaKey(key, theme)
      if (resolved) queries.add(resolved)
    }
    const band = (selector: string) => shimmerRule(selector, value as Record<string, unknown>)
    switch (condition.feature) {
      case `min-width`:
      case `max-width`: {
        const px = breakpointPx(condition.value, theme)
        if (px === null) break // an unknown token never matches
        const op = condition.feature === `min-width` ? `>=` : `<`
        rules.push(`@container xui (width ${op} ${px}px){${rule(sel, block)}${band(sel)}}`)
        break
      }
      case `min-height`:
      case `max-height`: {
        const px = breakpointPx(condition.value, theme)
        if (px === null) break
        const resolved = `@media (${condition.feature}: ${px}px)`
        queries.add(resolved)
        const q = `${scope}:where([data-xq~="${queryToken(resolved)}"]) .${cls}`
        rules.push(rule(q, block) + band(q))
        break
      }
      case `orientation`:
        queries.add(key)
        const q = `${scope}:where([data-xq~="${queryToken(key)}"]) .${cls}`
        rules.push(rule(q, block) + band(q))
        break
      case `hover`:
      case `prefers-reduced-motion`:
        rules.push(`@media (${condition.feature}: ${condition.value}){${rule(sel, block)}${band(sel)}}`)
        break
    }
  }
  const states: [string, string][] = [
    [`:hover`, `${sel}:hover,${sel}[data-xs~="hover"]`],
    [`:focus-visible`, `${sel}:focus-visible,${sel}[data-xs~="focus-visible"]`],
    [`:pressed`, `${sel}:active,${sel}[data-xs~="pressed"]`],
  ]
  for (const [key, selector] of states) {
    const value = style[key]
    if (typeof value !== `object` || value === null) continue
    const block = declarations(withVars(key, value as Record<string, unknown>), fonts, own)
    if (block.length) rules.push(rule(selector, block) + shimmerRule(selector, value as Record<string, unknown>))
  }
  return rules.join(``)
}

/** The node sheet for one tree (plus the template subtrees it renders). */
export function compileNodeSheet(roots: readonly UiNode[], surfaceId: string, theme?: ResolvedTheme, fonts: Record<string, FontSpec> = theme?.fonts ?? {}): NodeSheet {
  const scope = `.${surfaceClass(surfaceId)}`
  const chunks = new Map<string, string>()
  const queries = new Set<string>()
  const seen = new Set<string>()
  for (const root of roots) {
    for (const node of walkNodes(root)) {
      const style = node.style
      if (!style || seen.has(node.id)) continue
      seen.add(node.id)
      const chunk = nodeChunk(node, style, scope, theme, fonts)
      if (chunk.css) chunks.set(node.id, chunk.css)
      for (const q of chunk.queries) queries.add(q)
    }
  }
  return { css: layeredCss(chunks.values()), queries: [...queries], chunks }
}

/** VAPP-152: one `<style>` of the node sheet: the rules of some nodes. */
export interface NodeSheetBatch {
  readonly chunks: ReadonlyMap<string, string>
  readonly css: string
}

const sheetBatch = (chunks: ReadonlyMap<string, string>): NodeSheetBatch => ({ chunks, css: layeredCss(chunks.values()) })

/** VAPP-152: the node sheet as `<style>` batches, the last ones kept. A
 *  rewritten stylesheet makes Chromium re-match every node its rules
 *  select, so a batch whose rules are all unchanged keeps its OBJECT (the
 *  renderers leave its element alone), a batch with a changed or removed
 *  node is rewritten in place, and the new nodes' rules land in ONE new
 *  batch at the end (a streamed card restyles only itself). A batch at
 *  least as large as the one before it merges into it (a binary counter:
 *  at most log2(n) batches, each node rewritten O(log n) times). Rules of
 *  different nodes never select the same element, so the order of the
 *  batches does not change the cascade. */
export function nodeSheetBatches(prev: readonly NodeSheetBatch[], chunks: ReadonlyMap<string, string>): NodeSheetBatch[] {
  const out: NodeSheetBatch[] = []
  const placed = new Set<string>()
  for (const b of prev) {
    let dirty = false
    for (const [id, css] of b.chunks) {
      placed.add(id)
      if (chunks.get(id) !== css) dirty = true
    }
    if (!dirty) {
      out.push(b)
      continue
    }
    const kept = new Map<string, string>()
    for (const id of b.chunks.keys()) {
      const css = chunks.get(id)
      if (css !== undefined) kept.set(id, css)
    }
    if (kept.size) out.push(sheetBatch(kept))
  }
  const fresh = new Map<string, string>()
  for (const [id, css] of chunks) if (!placed.has(id)) fresh.set(id, css)
  if (fresh.size) out.push(sheetBatch(fresh))
  while (out.length > 1 && out[out.length - 1].chunks.size >= out[out.length - 2].chunks.size) {
    const last = out.pop()!
    const before = out.pop()!
    out.push(sheetBatch(new Map([...before.chunks, ...last.chunks])))
  }
  return out
}

/** The node sheet's CSS for one tree (the VAPP-87 entry point). */
export function nodeSheet(root: UiNode, surfaceId: string, fontsOrTheme: Record<string, FontSpec> | ResolvedTheme = {}, extra: readonly UiNode[] = []): string {
  const theme = isTheme(fontsOrTheme) ? fontsOrTheme : undefined
  const fonts = theme ? theme.fonts : (fontsOrTheme as Record<string, FontSpec>)
  return compileNodeSheet([root, ...extra], surfaceId, theme, fonts).css
}

function isTheme(v: unknown): v is ResolvedTheme {
  return typeof v === `object` && v !== null && `tokens` in v && `modes` in v
}
