// #VAPP-258 (fixtures/markdown-render.json): THE normal form every Markdown
// renderer's block model is compared in. The natives (Swift, Compose, gpui)
// parse into it directly (flat blocks, flag runs); the web tree flattens to
// it here. Adjacent runs with the same flags merge; false flags are omitted.

import type { MarkdownBlock, MarkdownInline, MarkdownList } from "./markdown"

export interface NormalRun {
  text: string
  bold?: true
  italic?: true
  code?: true
  strike?: true
  link?: string
}

export type NormalBlock =
  | { kind: `paragraph` | `quote`; runs: NormalRun[] }
  | { kind: `heading`; level: number; runs: NormalRun[] }
  | { kind: `listItem`; marker: string; task?: boolean; depth: number; runs: NormalRun[] }
  | { kind: `codeBlock`; text: string }
  | { kind: `table`; header: NormalRun[][]; rows: NormalRun[][][] }
  | { kind: `rule` }
  | { kind: `image`; src: string; alt: string }

type Flags = Omit<NormalRun, `text`>

/** Runs with equal flags merge, empty runs drop. */
export function mergeRuns(runs: readonly NormalRun[]): NormalRun[] {
  const out: NormalRun[] = []
  const key = (run: NormalRun) => JSON.stringify([run.bold, run.italic, run.code, run.strike, run.link])
  for (const run of runs) {
    if (!run.text) continue
    const last = out[out.length - 1]
    if (last && key(last) === key(run)) last.text += run.text
    else out.push({ ...run })
  }
  return out
}

function runs(nodes: readonly MarkdownInline[], flags: Flags = {}): NormalRun[] {
  const out: NormalRun[] = []
  for (const node of nodes) {
    if (typeof node === `string`) out.push({ text: node, ...flags })
    else if (node.type === `code`) out.push({ text: node.text, ...flags, code: true })
    else if (node.type === `strong`) out.push(...runs(node.children, { ...flags, bold: true }))
    else if (node.type === `em`) out.push(...runs(node.children, { ...flags, italic: true }))
    else if (node.type === `del`) out.push(...runs(node.children ?? [node.text], { ...flags, strike: true }))
    else out.push(...runs(node.children, { ...flags, link: node.dest }))
  }
  return out
}

const merged = (nodes: readonly MarkdownInline[]) => mergeRuns(runs(nodes))

function listItems(list: MarkdownList, depth: number, out: NormalBlock[]): void {
  list.items.forEach((item, i) => {
    const marker = list.ordered ? `${list.start + i}.` : `•`
    out.push({ kind: `listItem`, marker, ...(item.task !== undefined ? { task: item.task } : {}), depth, runs: merged(item.content) })
    for (const child of item.children) listItems(child, depth + 1, out)
  })
}

/** The web block tree in the normal form. A quote's paragraphs join into
 *  one quote block (the natives' flat quote). */
export function normalMarkdown(tree: readonly MarkdownBlock[]): NormalBlock[] {
  const out: NormalBlock[] = []
  for (const block of tree) {
    switch (block.type) {
      case `paragraph`:
        out.push({ kind: `paragraph`, runs: merged(block.children) })
        break
      case `heading`:
        out.push({ kind: `heading`, level: block.level, runs: merged(block.children) })
        break
      case `quote`: {
        const parts: NormalRun[] = []
        for (const child of normalMarkdown(block.children)) {
          if (!(`runs` in child)) continue
          if (parts.length) parts.push({ text: ` ` })
          parts.push(...child.runs)
        }
        out.push({ kind: `quote`, runs: mergeRuns(parts) })
        break
      }
      case `list`:
        listItems(block.list, 0, out)
        break
      case `codeBlock`:
        out.push({ kind: `codeBlock`, text: block.text })
        break
      case `table`:
        out.push({ kind: `table`, header: block.head.map(merged), rows: block.rows.map((row) => row.map(merged)) })
        break
      case `rule`:
        out.push({ kind: `rule` })
        break
      case `image`:
        out.push({ kind: `image`, src: block.src, alt: block.alt })
        break
    }
  }
  return out
}
