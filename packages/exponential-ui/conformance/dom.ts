// The WEB producer of a frame dump (conformance/dump.ts), run inside the
// page: every `[data-xui-id]` element of the surface's MAIN tree (overlay and
// toast layers excluded, `display: none` excluded) with its border box
// relative to the root node's (getBoundingClientRect, the browser's real
// fonts), minus collapsed 0×0 subtrees (dump.ts dropCollapsed). `text` = the text nodes the element owns (not a nested node's);
// `lines` (text components only) = the distinct line boxes of that text
// (Range client rects grouped by their vertical centre), `lh` = the computed
// line height.

import { dropCollapsed, roundNode, type CaseDump, type DumpNode } from "./dump"

/** The line boxes of a run of text nodes: client rects grouped by centre. */
function lineCount(texts: Text[]): number {
  const centres: number[] = []
  const range = document.createRange()
  for (const t of texts) {
    range.selectNodeContents(t)
    for (const r of Array.from(range.getClientRects())) {
      if (r.width === 0 && r.height === 0) continue
      const c = r.top + r.height / 2
      if (!centres.some((x) => Math.abs(x - c) < r.height / 2)) centres.push(c)
    }
  }
  return centres.length
}

/** The text nodes `el` owns (skipping nested placed nodes). */
function ownTexts(el: HTMLElement): Text[] {
  const out: Text[] = []
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT)
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const t = n as Text
    if (!t.data.trim()) continue
    if (t.parentElement?.closest(`[data-xui-id]`) !== el) continue
    out.push(t)
  }
  return out
}

export function domDump(surface: Element, textComponents: readonly string[]): CaseDump {
  const rootEl = surface.querySelector<HTMLElement>(`[data-xui-id]`)
  if (!rootEl) return { width: 0, height: 0, nodes: [] }
  const o = rootEl.getBoundingClientRect()
  const nodes: DumpNode[] = []
  const seen = new Set<string>()
  for (const el of Array.from(surface.querySelectorAll<HTMLElement>(`[data-xui-id]`))) {
    if (el.closest(`[data-xui-layer]`)) continue
    if (el.getClientRects().length === 0) continue
    const id = el.dataset.xuiId!
    if (seen.has(id)) continue
    seen.add(id)
    const r = el.getBoundingClientRect()
    const node: DumpNode = { id, component: el.dataset.xuiC ?? ``, x: r.left - o.left, y: r.top - o.top, w: r.width, h: r.height }
    if (el.dataset.xuiPart) node.part = el.dataset.xuiPart
    const parent = el.parentElement?.closest<HTMLElement>(`[data-xui-id]`)
    if (parent) node.parent = parent.dataset.xuiId
    const texts = ownTexts(el)
    if (texts.length) {
      node.text = texts
        .map((t) => t.data)
        .join(``)
        .replace(/\s+/g, ` `)
        .trim()
        .slice(0, 80)
      if (textComponents.includes(node.component)) {
        node.lines = lineCount(texts)
        const lh = parseFloat(getComputedStyle(el).lineHeight)
        if (Number.isFinite(lh)) node.lh = lh
      }
    }
    nodes.push(roundNode(node))
  }
  return { width: Math.round(o.width * 100) / 100, height: Math.round(o.height * 100) / 100, nodes: dropCollapsed(nodes) }
}
