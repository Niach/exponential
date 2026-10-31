// #EXP-1493: THE plan page ×4, the expanded form of THE plan card. A plan's
// markdown split into blocks (prose, h2 sections, picture groups), its
// outline, its counts line and where a session's waiting plan opens.
// fixtures/plan-page.json (its `$comment` is the spec); mirrors: desktop
// `domain::plan_page`, iOS ExpCore `PlanPage.swift`, Android
// `domain/PlanPage.kt`.

import fixture from "../fixtures/plan-page.json" with { type: "json" }

export const PLAN_PAGE_COPY = fixture.copy
export const PLAN_PAGE_LAYOUT = fixture.layout
export const PLAN_PAGE_MOSAIC = fixture.mosaic
export const PLAN_PAGE_TEST_IDS = fixture.testIds
export const PLAN_PAGE_ROUTE = fixture.route

export interface PlanPicture {
  alt: string
  src: string
}

export type PlanBlock =
  | { kind: `markdown`; text: string }
  | { kind: `section`; id: string; title: string }
  | { kind: `pictures`; pictures: PlanPicture[] }

export interface PlanSection {
  id: string
  title: string
  /** Pictures inside the section (grouped or single). */
  pictures: number
}

export interface PlanCounts {
  sections: number
  pictures: number
  questions: number
}

const PICTURE_LINE = /^\s*!\[([^\]]*)\]\(([^)\s]+)\)\s*$/
const PICTURE_ANY = /!\[[^\]]*\]\(([^)\s]+)\)/g
const H2 = /^##\s+(.+?)\s*#*\s*$/
const FENCE = /^\s*(```|~~~)/
const PENDING = `upload:`

/** A heading's anchor: lowercase, runs of anything but a-z0-9 = `-`. */
export function sectionSlug(title: string): string {
  const slug = title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, `-`)
    .replace(/^-+|-+$/g, ``)
  return slug || `section`
}

const plainTitle = (raw: string) =>
  raw
    .replace(/\[([^\]]*)\]\([^)]*\)/g, `$1`)
    .replaceAll(`**`, ``)
    .replaceAll(`__`, ``)
    .replaceAll("`", ``)
    .trim()

/** THE plan page's blocks (`blockCases`): the markdown in order, every `## `
 *  line outside a code fence = a `section` block (its anchor deduped `-2`,
 *  `-3`…), a run of two or more picture-only lines (blank lines between
 *  them allowed) = ONE `pictures` block; a lone picture stays in the prose.
 *  A pending `upload:` picture renders nothing: it is dropped. Prose blocks
 *  are trimmed of blank edge lines; an empty one is dropped. */
export function planBlocks(markdown: string): PlanBlock[] {
  const blocks: PlanBlock[] = []
  const seen = new Map<string, number>()
  let prose: string[] = []
  let run: PlanPicture[] = []
  let runLines: string[] = []
  let fenced = false

  const flushProse = () => {
    while (prose.length > 0 && prose[0]!.trim() === ``) prose.shift()
    while (prose.length > 0 && prose[prose.length - 1]!.trim() === ``) prose.pop()
    if (prose.length > 0) blocks.push({ kind: `markdown`, text: prose.join(`\n`) })
    prose = []
  }
  const flushRun = () => {
    if (run.length >= 2) {
      flushProse()
      blocks.push({ kind: `pictures`, pictures: run })
    } else {
      prose.push(...runLines)
    }
    run = []
    runLines = []
  }

  for (const line of markdown.split(`\n`)) {
    if (FENCE.test(line)) {
      flushRun()
      fenced = !fenced
      prose.push(line)
      continue
    }
    if (fenced) {
      prose.push(line)
      continue
    }
    const picture = PICTURE_LINE.exec(line)
    if (picture) {
      if (picture[2]!.startsWith(PENDING)) continue
      run.push({ alt: picture[1]!, src: picture[2]! })
      runLines.push(line)
      continue
    }
    if (line.trim() === `` && run.length > 0) {
      runLines.push(line)
      continue
    }
    flushRun()
    const heading = H2.exec(line)
    if (heading) {
      flushProse()
      const title = plainTitle(heading[1]!)
      const base = sectionSlug(title)
      const n = (seen.get(base) ?? 0) + 1
      seen.set(base, n)
      blocks.push({ kind: `section`, id: n === 1 ? base : `${base}-${n}`, title })
      continue
    }
    prose.push(line.replace(PICTURE_ANY, (whole, src: string) => (src.startsWith(PENDING) ? `` : whole)))
  }
  flushRun()
  flushProse()
  return blocks
}

/** The pictures a block shows (a fenced code block shows none). */
function picturesIn(block: PlanBlock): number {
  if (block.kind === `pictures`) return block.pictures.length
  if (block.kind !== `markdown`) return 0
  let fenced = false
  let count = 0
  for (const line of block.text.split(`\n`)) {
    if (FENCE.test(line)) fenced = !fenced
    else if (!fenced) count += [...line.matchAll(PICTURE_ANY)].filter((match) => !match[1]!.startsWith(PENDING)).length
  }
  return count
}

/** THE outline (`outlineCases`): one row per `section` block, in order, with
 *  the pictures between it and the next section. */
export function planOutline(markdown: string): PlanSection[] {
  const out: PlanSection[] = []
  for (const block of planBlocks(markdown)) {
    if (block.kind === `section`) out.push({ id: block.id, title: block.title, pictures: 0 })
    else if (out.length > 0) out[out.length - 1]!.pictures += picturesIn(block)
  }
  return out
}

/** A plan card's counts (`countCases`): its h2 sections, its RESOLVED
 *  pictures (attachmentId set), its questions. */
export function planCounts(card: unknown): PlanCounts {
  const c = (card && typeof card === `object` ? card : {}) as Record<string, unknown>
  const pictures = Array.isArray(c.pictures)
    ? c.pictures.filter((p) => p && typeof p === `object` && typeof (p as { attachmentId?: unknown }).attachmentId === `string`).length
    : 0
  return {
    sections: planOutline(typeof c.plan === `string` ? c.plan : ``).length,
    pictures,
    questions: Array.isArray(c.questions) ? c.questions.length : 0,
  }
}

/** "5 sections · 10 pictures · 7 questions": zero parts left out, the
 *  singular for one; nothing = ``. */
export function planCountsLine(counts: PlanCounts): string {
  const parts: string[] = []
  for (const key of [`sections`, `pictures`, `questions`] as const) {
    const n = counts[key]
    if (n > 0) parts.push(`${n} ${fixture.copy.counts[key][n === 1 ? 0 : 1]}`)
  }
  return parts.join(` · `)
}

/** The outline's questions row trailing text: "2 of 7 answered". */
export function answeredLine(answered: number, total: number): string {
  return fixture.copy.answered.replace(`{answered}`, String(answered)).replace(`{total}`, String(total))
}

export interface MosaicTile {
  /** The picture's index in the group. */
  index: number
  /** `large` = the left tile two rows tall, `half` = one of a pair, `small` = a right tile. */
  size: `large` | `half` | `small`
  /** "+N" on the last small tile when the group has more than it shows. */
  more: number
}

/** THE wide picture group (eUI `Mosaic`, `mosaicCases`): two = two halves;
 *  three or more = the first large on the left, the next two small stacked on
 *  the right, the last of them carrying "+N" (N = the pictures not shown). */
export function mosaicTiles(count: number): MosaicTile[] {
  if (count <= 0) return []
  if (count === 1) return [{ index: 0, size: `large`, more: 0 }]
  if (count === 2) return [0, 1].map((index) => ({ index, size: `half` as const, more: 0 }))
  return [
    { index: 0, size: `large`, more: 0 },
    { index: 1, size: `small`, more: 0 },
    { index: 2, size: `small`, more: count - 3 },
  ]
}

export interface PlanPageMessage {
  id: string
  sessionId: string
  authorSessionId: string | null
  /** Epoch ms. */
  createdAt: number
  card: unknown
}

/** Where a session's waiting plan opens (`targetCases`): the NEWEST plan card
 *  the session posted itself (author = session, no engine `questionId`) with
 *  no decision; null = none (the host opens the session). */
export function planPageFor(sessionId: string, messages: readonly PlanPageMessage[]): string | null {
  let best: PlanPageMessage | null = null
  for (const message of messages) {
    if (message.sessionId !== sessionId || message.authorSessionId !== sessionId) continue
    const card = message.card as Record<string, unknown> | null
    if (!card || card.kind !== `plan` || card.decision != null || typeof card.questionId === `string`) continue
    if (!best || message.createdAt > best.createdAt || (message.createdAt === best.createdAt && message.id > best.id)) {
      best = message
    }
  }
  return best?.id ?? null
}

/** The page's path (web); natives push the same page. */
export function planPagePath(teamSlug: string, sessionId: string, messageId: string): string {
  return fixture.route
    .replace(`$teamSlug`, teamSlug)
    .replace(`$sessionId`, sessionId)
    .replace(`$messageId`, messageId)
}
