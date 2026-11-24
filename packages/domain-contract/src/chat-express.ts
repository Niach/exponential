// #EXP-2067 (fixtures/chat-express.json): Express blocks inline in an
// agent's text ×4. The TS reference every mirror replays (desktop
// `domain::chat_express`, ExpCore `ChatExpress`, Android `ChatExpress`).
//
// The SPLIT and the COMPILE are the eui SDK's (`unwrap` + an unwrapped
// `ExpressDecoder` per block), injected as an `ExpressSdk` so this module
// stays free of the wasm: the web passes `@exponential_app/eui/express`,
// the tests the same over the committed wasm. Everything that reads an
// agent text WITHOUT drawing it (previews, notifications, search,
// read-aloud, copy) uses the pure `plainProse` instead, no SDK needed.
import fixture from "../fixtures/chat-express.json" with { type: "json" }

export const CHAT_EXPRESS = fixture
export const EXPRESS_LIMITS = fixture.limits
export const EXPRESS_COPY = fixture.copy
export const EXPRESS_REPLY_EVENT = fixture.replyEvent
export const EXPRESS_PROMPT_PREAMBLE = fixture.prompt.preamble
const GATE = new RegExp(fixture.gate, `m`)

/** One part of `unwrap` (the wasm / eui-core / FFI JSON shape). */
export type RawPart = { text: string } | { a2ui_raw: string; is_final?: boolean }

/** What a block's decoder returns per push (the SDK's `Decoded`). */
export interface BlockDecoded {
  messages: unknown[]
  issues: { at: number; message: string }[]
}

/** The SDK, injected. */
export interface ExpressSdk {
  unwrap(text: string): RawPart[]
  /** An UNWRAPPED Express stream whose default surface id is `surfaceId`. */
  decoder(surfaceId: string): { push(chunk: string): BlockDecoded; end(): BlockDecoded }
}

export type BlockStatus = `open` | `ok` | `failed` | `tooMany` | `tooLarge`

export type Segment =
  | { kind: `prose`; text: string }
  | { kind: `block`; key: string; raw: string; closed: boolean }

export interface BlockState {
  kind: `block`
  key: string
  raw: string
  closed: boolean
  status: BlockStatus
  /** The SDK's category (`ParseError` | `ValidationError` | `CatalogError`)
   *  or `Unclosed`; only on `failed`. */
  error?: string
  /** The SDK's own words (never fixture-locked). */
  errorMessage?: string
  /** Surface ids the block's stream created and did not delete. */
  surfaces: string[]
  /** EVERY A2UI message of the block's stream since it (re)started: what
   *  its renderer applies (a restart bumps `generation`). */
  messages: unknown[]
  generation: number
}

export type RowSegment = { kind: `prose`; text: string } | BlockState

/** A row's text holds blocks: one of its lines starts with the open tag. */
export function hasExpressBlock(text: string): boolean {
  return GATE.test(text)
}

const TAG = `<a2ui>`

/** While a row streams, a last line that is a proper prefix of `<a2ui>`
 *  is held back (the tag may still be arriving). */
export function withoutHeldTail(text: string, final: boolean): string {
  if (final) return text
  const nl = text.lastIndexOf(`\n`)
  const last = text.slice(nl + 1).trimStart()
  if (last.length > 0 && last.length < TAG.length && TAG.startsWith(last)) return text.slice(0, nl + 1).replace(/\n$/, ``)
  return text
}

/** The key of a synced row (`m:<messages.id>`). */
export const syncedRowKey = (messageId: string): string => `m:${messageId}`
/** The key of a feed narration row: its messageId, else its first seq. */
export const feedRowKey = (sessionId: string, messageId: string | null | undefined, firstSeq: number): string =>
  messageId ? `n:${sessionId}:${messageId}` : `e:${sessionId}:${firstSeq}`

/** `segmentCases` without the compile: null when the gate fails. */
export function expressSegments(text: string, rowKey: string, sdk: Pick<ExpressSdk, `unwrap`>): Segment[] | null {
  if (!hasExpressBlock(text)) return null
  let n = 0
  const out: Segment[] = []
  for (const part of sdk.unwrap(text)) {
    if (`text` in part) {
      if (part.text.trim()) out.push({ kind: `prose`, text: part.text })
    } else {
      n += 1
      out.push({ kind: `block`, key: `${rowKey}#${n}`, raw: part.a2ui_raw, closed: part.is_final !== false })
    }
  }
  return out
}

const ISSUE_PREFIX: Record<string, string> = {
  "parse error: ": `ParseError`,
  "validation error: ": `ValidationError`,
  "catalog error: ": `CatalogError`,
}

/** A decoder issue's message (`<category text>: <message>`, the SDK's
 *  form on every platform) → the category and the SDK's words. */
export function issueCategory(message: string): { category: string; message: string } {
  for (const [prefix, category] of Object.entries(ISSUE_PREFIX)) {
    if (message.startsWith(prefix)) return { category, message: message.slice(prefix.length) }
  }
  return { category: `ParseError`, message }
}

const utf8 = (s: string): number => new TextEncoder().encode(s).length

/** The surfaces a block's messages leave and its component ids per surface. */
function tally(messages: readonly unknown[]): { surfaces: string[]; components: number } {
  const surfaces: string[] = []
  const comps = new Map<string, Set<string>>()
  for (const m of messages as Record<string, Record<string, unknown> | undefined>[]) {
    const create = m.createSurface as { surfaceId: string; components?: { id: string }[] } | undefined
    const update = m.updateComponents as { surfaceId: string; components?: { id: string }[] } | undefined
    if (create) {
      surfaces.push(create.surfaceId)
      comps.set(create.surfaceId, new Set())
    }
    const c = create ?? update
    if (c) for (const k of c.components ?? []) comps.get(c.surfaceId)?.add(k.id)
    const del = m.deleteSurface as { surfaceId: string } | undefined
    if (del) {
      const i = surfaces.indexOf(del.surfaceId)
      if (i >= 0) surfaces.splice(i, 1)
      comps.delete(del.surfaceId)
    }
  }
  let components = 0
  for (const s of comps.values()) components += s.size
  return { surfaces, components }
}

interface LiveBlock {
  raw: string
  decoder: { push(chunk: string): BlockDecoded; end(): BlockDecoded }
  messages: unknown[]
  issue: { at: number; message: string } | null
  ended: boolean
  generation: number
}

/** One row's blocks across its updates (`streamCases`): `update(text,
 *  final)` re-splits the text, pushes only a block's new suffix, restarts a
 *  block whose raw changed otherwise, and ends a block once it is closed
 *  or the row is final. Any chunking reaches the states `segmentCases`
 *  lists for the text so far. */
export class ExpressRow {
  private blocks = new Map<string, LiveBlock>()

  constructor(
    private readonly rowKey: string,
    private readonly sdk: ExpressSdk
  ) {}

  update(text: string, final: boolean): RowSegment[] | null {
    const shown = withoutHeldTail(text, final)
    const segments = expressSegments(shown, this.rowKey, this.sdk)
    if (segments === null) {
      this.blocks.clear()
      if (shown === text) return null
      // a held tail: the prose before it, the tag still arriving
      return shown.trim() ? [{ kind: `prose`, text: shown.trim() }] : []
    }
    let index = 0
    const seen = new Set<string>()
    const out: RowSegment[] = segments.map((seg) => {
      if (seg.kind === `prose`) return seg
      seen.add(seg.key)
      return this.block(seg, index++, final)
    })
    for (const key of [...this.blocks.keys()]) if (!seen.has(key)) this.blocks.delete(key)
    return out
  }

  private block(seg: Extract<Segment, { kind: `block` }>, index: number, final: boolean): BlockState {
    const base = { kind: `block` as const, key: seg.key, raw: seg.raw, closed: seg.closed }
    const limits = EXPRESS_LIMITS
    if (index >= limits.blocksPerRowMax) return { ...base, status: `tooMany`, surfaces: [], messages: [], generation: 0 }
    if (utf8(seg.raw) > limits.blockMaxBytes) return { ...base, status: `tooLarge`, surfaces: [], messages: [], generation: 0 }
    let live = this.blocks.get(seg.key)
    if (live && !(seg.raw.startsWith(live.raw) && !(live.ended && seg.raw !== live.raw))) {
      live = this.fresh(seg.key, live.generation + 1)
    }
    if (!live) live = this.fresh(seg.key, 0)
    this.blocks.set(seg.key, live)
    if (!live.ended && seg.raw.length > live.raw.length) {
      this.take(live, live.decoder.push(seg.raw.slice(live.raw.length)))
      live.raw = seg.raw
    }
    if (!live.ended && (seg.closed || final)) {
      this.take(live, live.decoder.end())
      live.ended = true
    }
    const { surfaces, components } = tally(live.messages)
    if (surfaces.length > limits.blockMaxSurfaces || components > limits.blockMaxComponents) {
      return { ...base, status: `tooLarge`, surfaces: [], messages: [], generation: live.generation }
    }
    const common = { surfaces, messages: live.messages, generation: live.generation }
    if (live.issue) {
      const { category, message } = issueCategory(live.issue.message)
      return { ...base, ...common, status: `failed`, error: category, errorMessage: message }
    }
    if (!seg.closed) {
      return final
        ? { ...base, ...common, status: `failed`, error: `Unclosed`, errorMessage: UNCLOSED_MESSAGE }
        : { ...base, ...common, status: `open` }
    }
    return { ...base, ...common, status: `ok` }
  }

  private fresh(key: string, generation: number): LiveBlock {
    return { raw: ``, decoder: this.sdk.decoder(key), messages: [], issue: null, ended: false, generation }
  }

  private take(live: LiveBlock, d: BlockDecoded): void {
    if (live.issue) return
    live.messages.push(...d.messages)
    if (d.issues.length) live.issue = d.issues[0]!
  }
}

const UNCLOSED_MESSAGE = `The block has no </a2ui> line.`

/** `segmentCases` in one go (a fresh row). */
export function expressRow(text: string, rowKey: string, final: boolean, sdk: ExpressSdk): RowSegment[] | null {
  return new ExpressRow(rowKey, sdk).update(text, final)
}

/** The drawn prose of a row (`segmentCases.prose`): its prose segments
 *  joined by a blank line; the whole text when it holds no block. */
export function expressProse(segments: readonly RowSegment[] | null, text: string): string {
  if (segments === null) return text
  return segments
    .filter((s): s is { kind: `prose`; text: string } => s.kind === `prose`)
    .map((s) => s.text)
    .join(`\n\n`)
}

const OPEN_LINE = new RegExp(`^${fixture.gate.slice(1)}`)
const FENCE = /^[ \t]*(```|~~~)[\w-]*[ \t]*$/
const CLOSE_LINE = /^[ \t]*<\/a2ui>[ \t]*$/

/** The PURE prose (`plainProseCases`) for every read that does not draw:
 *  previews, notifications, search, read-aloud, copy. Lines from an open
 *  tag line through the next `</a2ui>` line (or the end) go, with a fence
 *  line right before or after; the remaining runs, trimmed, joined by a
 *  blank line. No SDK: a list never loads the compiler. */
export function plainProse(text: string): string {
  if (!hasExpressBlock(text)) return text
  const lines = text.split(`\n`)
  const keep: (string | null)[] = []
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!
    if (!OPEN_LINE.test(line)) {
      keep.push(line)
      continue
    }
    // a fence line right before the block goes with it
    if (keep.length && keep[keep.length - 1] !== null && FENCE.test(keep[keep.length - 1]!)) keep.pop()
    const rest = line.replace(OPEN_LINE, ``)
    let j = i
    if (!/<\/a2ui>/.test(rest)) {
      j = i + 1
      while (j < lines.length && !CLOSE_LINE.test(lines[j]!)) j++
    }
    if (j + 1 < lines.length && FENCE.test(lines[j + 1]!)) j++
    keep.push(null)
    i = j
  }
  const runs: string[] = []
  let run: string[] = []
  for (const l of keep) {
    if (l === null) {
      runs.push(run.join(`\n`))
      run = []
    } else run.push(l)
  }
  runs.push(run.join(`\n`))
  return runs
    .map((r) => r.trim())
    .filter((r) => r !== ``)
    .join(`\n\n`)
}

// ── Actions ────────────────────────────────────────────────────────────────

/** THE `action` message card. */
export interface ActionCard {
  kind: `action`
  name: string
  context: Record<string, unknown> | null
  surfaceId: string
  block: string | null
  messageId: string | null
}

export interface ActionOrigin {
  surfaceId: string
  block: string | null
  messageId: string | null
}

/** An action event as the renderer hands it over. */
export interface ActionEvent {
  name?: unknown
  context?: unknown
  userMessage?: unknown
}

const trimmed = (v: unknown): string => (typeof v === `string` ? v.trim() : ``)

/** `actionCases`: the person message an action posts (body + card), or
 *  `tooLarge` (nothing posted, THE failed-action toast). */
export function surfaceAction(event: ActionEvent, origin: ActionOrigin): { body: string; card: ActionCard } | `tooLarge` | null {
  if (typeof event.name !== `string` || event.name === ``) return null
  const name = event.name
  const context =
    event.context && typeof event.context === `object` && !Array.isArray(event.context) && Object.keys(event.context).length > 0
      ? (event.context as Record<string, unknown>)
      : null
  if ([...name].length > EXPRESS_LIMITS.actionNameMax) return `tooLarge`
  if (context && utf8(JSON.stringify(context)) > EXPRESS_LIMITS.actionContextMaxBytes) return `tooLarge`
  const user = trimmed(event.userMessage)
  const body = name === EXPRESS_REPLY_EVENT ? user || trimmed((context as { text?: unknown } | null)?.text) : user
  return {
    body,
    card: { kind: `action`, name, context, surfaceId: origin.surfaceId, block: origin.block, messageId: origin.messageId },
  }
}

/** An exponential_show surface's origin (`show:<messages.id>`). */
export const showActionOrigin = (messageId: string): ActionOrigin => ({ surfaceId: `show:${messageId}`, block: null, messageId })

const ACTION_LEAD = `[action] `

/** `inputCases`: the ONE input the device hands the agent. */
export function actionInput(card: ActionCard, body: string): string {
  const head = JSON.stringify({ name: card.name, surfaceId: card.surfaceId, block: card.block, messageId: card.messageId, context: card.context })
  return `${ACTION_LEAD}${head}${body ? `\n${body}` : ``}`
}

/** A feed input back to its card and body (null = not an action input). */
export function parseActionInput(text: string): { card: ActionCard; body: string } | null {
  if (!text.startsWith(ACTION_LEAD)) return null
  const nl = text.indexOf(`\n`)
  const head = nl < 0 ? text.slice(ACTION_LEAD.length) : text.slice(ACTION_LEAD.length, nl)
  try {
    const v = JSON.parse(head) as Record<string, unknown>
    if (typeof v.name !== `string` || typeof v.surfaceId !== `string`) return null
    const context = v.context && typeof v.context === `object` && !Array.isArray(v.context) ? (v.context as Record<string, unknown>) : null
    return {
      card: {
        kind: `action`,
        name: v.name,
        context,
        surfaceId: v.surfaceId,
        block: typeof v.block === `string` ? v.block : null,
        messageId: typeof v.messageId === `string` ? v.messageId : null,
      },
      body: nl < 0 ? `` : text.slice(nl + 1),
    }
  } catch {
    return null
  }
}

/** The marker's words of an action row without a body. */
export function actionMarker(name: string, author: string | null): string {
  return EXPRESS_COPY.actionMarker.replace(`{author}`, author ?? EXPRESS_COPY.you).replace(`{name}`, name)
}

// ── Failed blocks tell the agent ──────────────────────────────────────────

/** `failureNoteCases`: the lines a finished turn's failed blocks leave for
 *  the agent's next input (null = none). `turnTexts` = the turn's
 *  main-lane narration texts in order, each final. */
export function failureNote(turnTexts: readonly string[], sdk: ExpressSdk): string | null {
  const limits = EXPRESS_LIMITS
  const notes: string[] = []
  let n = 0
  for (const text of turnTexts) {
    if (!hasExpressBlock(text)) continue
    let inRow = 0
    for (const part of sdk.unwrap(text)) {
      if (`text` in part) continue
      n += 1
      inRow += 1
      const raw = part.a2ui_raw
      const closed = part.is_final !== false
      const err =
        inRow > limits.blocksPerRowMax
          ? { category: `Limit`, message: `More than ${limits.blocksPerRowMax} blocks in one message.`, line: 1 }
          : utf8(raw) > limits.blockMaxBytes
            ? { category: `Limit`, message: `More than ${limits.blockMaxBytes} bytes in one block.`, line: 1 }
            : blockError(raw, closed, `note#${n}`, sdk)
      if (!err) continue
      let line = (raw.split(`\n`)[err.line - 1] ?? ``).trim()
      const chars = [...line]
      if (chars.length > limits.noteLineMax) line = `${chars.slice(0, limits.noteLineMax - 1).join(``)}…`
      notes.push(`[view] Block ${n} of your last turn did not render (${err.category}): ${err.message} Line ${err.line}: ${line}`)
    }
  }
  return notes.length ? notes.join(`\n`) : null
}

/** A block fed one line at a time: the first error and its line. */
function blockError(raw: string, closed: boolean, key: string, sdk: ExpressSdk): { category: string; message: string; line: number } | null {
  const limits = EXPRESS_LIMITS
  const lines = raw.split(`\n`)
  const decoder = sdk.decoder(key)
  const messages: unknown[] = []
  const check = (d: BlockDecoded, line: number) => {
    for (const m of d.messages) {
      messages.push(m)
      const { surfaces, components } = tally(messages)
      if (surfaces.length > limits.blockMaxSurfaces) return { category: `Limit`, message: `More than ${limits.blockMaxSurfaces} surfaces in one block.`, line }
      if (components > limits.blockMaxComponents) return { category: `Limit`, message: `More than ${limits.blockMaxComponents} components in one block.`, line }
    }
    const issue = d.issues[0]
    if (issue) return { ...issueCategory(issue.message), line }
    return null
  }
  for (let i = 0; i < lines.length; i++) {
    const err = check(decoder.push(i < lines.length - 1 ? `${lines[i]}\n` : lines[i]!), i + 1)
    if (err) return err
  }
  const err = check(decoder.end(), lines.length)
  if (err) return err
  if (!closed) return { category: `Unclosed`, message: UNCLOSED_MESSAGE, line: lines.length }
  return null
}
