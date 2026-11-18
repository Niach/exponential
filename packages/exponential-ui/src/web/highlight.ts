// #EDE-95 (fixtures/text-highlight.json): the web half of genUI's search
// marks. `markdownHighlightTexts` = the texts a Markdown searches, one per
// block of the normal form (a paragraph, heading, list item or quote; each
// table cell, the header first; never a code block, image, attachment or
// rule), a chip run as ONE `HIGHLIGHT_BARRIER` (chips stay whole), a link
// as its label (never its target). `nodeHighlightCount` = a node's matches
// so a host can number them across a surface. `paintHighlights` paints a
// rendered Text or Markdown through the CSS Custom Highlight API (no DOM
// change, so React and Solid keep their trees): `::highlight(xui-match)`
// and `::highlight(xui-match-current)` fill with `$color.match` /
// `$color.matchCurrent` (base-css). A browser without the API paints none.

import { HIGHLIGHT_BARRIER, highlightCount, highlightLocalCurrent, highlightRanges, highlightSpec, type HighlightSpec } from "../highlight"
import { parseMarkdown, type MarkdownBlock } from "./markdown"
import { normalMarkdown, type NormalBlock, type NormalRun } from "./markdown-normal"

/** The CSS highlight names (base-css paints them). */
export const HIGHLIGHT_NAMES = { match: `xui-match`, current: `xui-match-current` } as const

function runsText(runs: readonly NormalRun[]): string {
  return runs.map((run) => (run.chip ? HIGHLIGHT_BARRIER : run.text)).join(``)
}

/** The searched text of each block, in document order. */
export function markdownHighlightTexts(blocks: readonly NormalBlock[]): string[] {
  const out: string[] = []
  for (const block of blocks) {
    if (`runs` in block) out.push(runsText(block.runs))
    else if (block.kind === `table`) {
      for (const cell of block.header) out.push(runsText(cell))
      for (const row of block.rows) for (const cell of row) out.push(runsText(cell))
    }
  }
  return out
}

/** A Markdown's matches (`tree` = the host-linked tree when the host has
 *  text links, so its chips count as barriers). */
export function markdownHighlightCount(text: string, query: string, tree?: readonly MarkdownBlock[]): number {
  return markdownHighlightTexts(normalMarkdown(tree ?? parseMarkdown(text))).reduce((n, t) => n + highlightCount(t, query), 0)
}

/** A node's matches for its own `highlight` (0 without one): Text = its
 *  text, Markdown and MessageBubble = the markdown, Row = title + subtitle. */
export function nodeHighlightCount(component: string, props: Record<string, unknown>): number {
  const spec = highlightSpec(props.highlight)
  if (!spec) return 0
  const str = (v: unknown) => (typeof v === `string` ? v : ``)
  switch (component) {
    case `Text`:
      return highlightCount(str(props.text), spec.query)
    case `Markdown`:
    case `MessageBubble`:
      return markdownHighlightCount(str(props.text), spec.query)
    case `Row`:
      return highlightCount(str(props.title), spec.query) + highlightCount(str(props.subtitle), spec.query)
    default:
      return 0
  }
}

// ---- painting --------------------------------------------------------------

interface Piece {
  node: Text | null
  /** Offset of the piece in the block's text. */
  at: number
  text: string
}

const BLOCK = `P,H1,H2,H3,H4,H5,H6,LI,TH,TD,BLOCKQUOTE`.split(`,`)
const CHIP = `.xui-md-chip`

/** The block a markdown text node belongs to (null = not searched): the
 *  nearest list item or table cell, else the OUTERMOST quote, else the
 *  nearest paragraph or heading; a code block never. */
function blockOf(node: Node, root: Element): Element | null {
  let nearest: Element | null = null
  let quote: Element | null = null
  for (let el = node.parentElement; el && el !== root; el = el.parentElement) {
    if (el.tagName === `PRE`) return null
    if (!BLOCK.includes(el.tagName)) continue
    if (el.tagName === `LI` || el.tagName === `TH` || el.tagName === `TD`) return quote ?? el
    if (el.tagName === `BLOCKQUOTE`) quote = el
    else nearest ??= el
  }
  return quote ?? nearest
}

/** The pieces of each searched block of a rendered Markdown (or the one
 *  block of a Text), in document order. */
function blocksOf(root: Element, markdown: boolean): Piece[][] {
  const groups = new Map<Element, Piece[]>()
  const order: Piece[][] = []
  const lengths = new Map<Piece[], number>()
  const lastPara = new Map<Piece[], Element | null>()
  const seenChips = new Set<Element>()
  const doc = root.ownerDocument
  const walker = doc.createTreeWalker(root, 1 | 4)
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const el = n.nodeType === 1 ? (n as Element) : null
    if (el && el.tagName !== `BR`) continue
    const block = markdown ? blockOf(n, root) : root
    if (!block) continue
    let group = groups.get(block)
    if (!group) {
      group = []
      groups.set(block, group)
      order.push(group)
      lengths.set(group, 0)
    }
    const push = (node: Text | null, text: string) => {
      const at = lengths.get(group!)!
      group!.push({ node, at, text })
      lengths.set(group!, at + text.length)
    }
    if (el) {
      push(null, `\n`)
      continue
    }
    const text = n as Text
    const chip = markdown ? text.parentElement?.closest(CHIP) : null
    if (chip && root.contains(chip)) {
      if (!seenChips.has(chip)) {
        seenChips.add(chip)
        push(null, HIGHLIGHT_BARRIER)
      }
      continue
    }
    // A task item's checkbox gap is no text of the item.
    if (text.data === ` ` && (text.previousSibling as Element | null)?.tagName === `INPUT`) continue
    // A quote's paragraphs join with one space (the normal form).
    if (block.tagName === `BLOCKQUOTE`) {
      const para = text.parentElement?.closest(`p`) ?? null
      const last = lastPara.get(group)
      if (last !== undefined && para !== last && lengths.get(group)! > 0) push(null, ` `)
      lastPara.set(group, para)
    }
    push(text, text.data)
  }
  return order
}

interface Registry {
  highlights: Map<string, { add(r: unknown): void; delete(r: unknown): boolean; size: number }>
}

/** The document's highlight registry, made on first use (null = none). */
function registry(doc: Document): Registry | null {
  const win = doc.defaultView as (Window & { CSS?: { highlights?: Map<string, unknown> }; Highlight?: new () => unknown }) | null
  const reg = win?.CSS?.highlights
  if (!reg || typeof win?.Highlight !== `function`) return null
  for (const name of Object.values(HIGHLIGHT_NAMES)) if (!reg.has(name)) reg.set(name, new win.Highlight())
  return { highlights: reg as Registry[`highlights`] }
}

/** A DOM range over the block's pieces for [start, end). */
function rangeOf(doc: Document, pieces: readonly Piece[], start: number, end: number): Range | null {
  let from: { node: Text; offset: number } | null = null
  let to: { node: Text; offset: number } | null = null
  for (const p of pieces) {
    if (!p.node) continue
    const pEnd = p.at + p.text.length
    if (!from && start < pEnd) from = { node: p.node, offset: Math.max(0, start - p.at) }
    if (end > p.at && end <= pEnd) to = { node: p.node, offset: end - p.at }
    else if (end > pEnd && p.at < end) to = { node: p.node, offset: p.text.length }
  }
  if (!from || !to) return null
  const range = doc.createRange()
  range.setStart(from.node, from.offset)
  range.setEnd(to.node, to.offset)
  return range
}

/** The match ranges a rendered node paints: `current` = the local index
 *  of the current match (-1 = none). Exposed for tests. */
export function highlightPieces(root: Element, spec: HighlightSpec, markdown: boolean): { block: number; start: number; end: number; current: boolean }[] {
  const local = highlightLocalCurrent(spec)
  const out: { block: number; start: number; end: number; current: boolean }[] = []
  blocksOf(root, markdown).forEach((pieces, block) => {
    const text = pieces.map((p) => p.text).join(``)
    for (const r of highlightRanges(text, spec.query)) out.push({ block, start: r.start, end: r.end, current: out.length === local })
  })
  return out
}

/** Paints the matches of `highlight` (the raw prop) in a rendered Text
 *  (`markdown` false: its whole text) or Markdown; returns the cleanup. */
export function paintHighlights(root: Element, highlight: unknown, markdown: boolean): () => void {
  const spec = highlightSpec(highlight)
  const doc = root.ownerDocument
  const reg = spec ? registry(doc) : null
  if (!spec || !reg) return () => {}
  const blocks = blocksOf(root, markdown)
  const matches = highlightPieces(root, spec, markdown)
  const added: [string, Range][] = []
  for (const m of matches) {
    const range = rangeOf(doc, blocks[m.block]!, m.start, m.end)
    if (!range) continue
    const name = m.current ? HIGHLIGHT_NAMES.current : HIGHLIGHT_NAMES.match
    reg.highlights.get(name)!.add(range)
    added.push([name, range])
  }
  return () => {
    for (const [name, range] of added) reg.highlights.get(name)?.delete(range)
  }
}
