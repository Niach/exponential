// NEW-53: exponentialTalk's voice settings and spoken-reply rules. Pure and
// byte-locked by `fixtures/voice.json`; the desktop mirror
// `domain::voice` replays the same cases, the phones mirror `copy`.

import fixture from "../fixtures/voice.json" with { type: "json" }

export const VOICE_COPY = fixture.copy
export const VOICE_TEST_IDS = fixture.testIds
export const VOICE_MAX_CHARS = fixture.maxChars

export interface VoiceMessageLike {
  authorKind?: string | null
  body?: string | null
  card?: unknown
  authorSessionId?: string | null
  /** The session the row belongs to (resolved through its conversation). */
  sessionId?: string | null
  createdAt?: string | null
}

export interface VoiceContext {
  /** The member's conversation session with the assistant agent. */
  assistantSessionId: string | null
  speakReplies: boolean
  /** When the watcher armed; older rows are a backfill. */
  armedAt: string
}

/** Whether a synced message row is read aloud. */
export function shouldSpeak(message: VoiceMessageLike, context: VoiceContext): boolean {
  if (!context.speakReplies || !context.assistantSessionId) return false
  if (message.authorKind !== `agent`) return false
  if (message.card) return false
  if (!message.body || message.body.trim() === ``) return false
  if (message.authorSessionId) return false
  if (message.sessionId !== context.assistantSessionId) return false
  const created = Date.parse(message.createdAt ?? ``)
  const armed = Date.parse(context.armedAt)
  if (Number.isNaN(created) || Number.isNaN(armed)) return false
  return created >= armed
}

const ONES = [
  `zero`, `one`, `two`, `three`, `four`, `five`, `six`, `seven`, `eight`, `nine`,
  `ten`, `eleven`, `twelve`, `thirteen`, `fourteen`, `fifteen`, `sixteen`,
  `seventeen`, `eighteen`, `nineteen`,
]
const TENS = [``, ``, `twenty`, `thirty`, `forty`, `fifty`, `sixty`, `seventy`, `eighty`, `ninety`]

function belowHundred(n: number): string {
  if (n < 20) return ONES[n]
  const tens = TENS[Math.floor(n / 10)]
  return n % 10 === 0 ? tens : `${tens}-${ONES[n % 10]}`
}

function belowThousand(n: number): string {
  const hundreds = Math.floor(n / 100)
  const rest = n % 100
  const parts: string[] = []
  if (hundreds > 0) parts.push(`${ONES[hundreds]} hundred`)
  if (rest > 0 || hundreds === 0) parts.push(belowHundred(rest))
  return parts.join(` `)
}

function digitsWords(digits: string): string {
  return digits.split(``).map((d) => ONES[Number(d)]).join(` `)
}

/** English cardinal words for a number written with digits (an optional
 * decimal part reads `point` and then digit by digit). */
export function numberWords(text: string): string {
  const [whole, fraction] = text.split(`.`)
  let words: string
  if (whole.length > 9) {
    words = digitsWords(whole)
  } else {
    let n = Number(whole)
    if (n === 0) {
      words = `zero`
    } else {
      const parts: string[] = []
      const millions = Math.floor(n / 1_000_000)
      n %= 1_000_000
      const thousands = Math.floor(n / 1_000)
      n %= 1_000
      if (millions > 0) parts.push(`${belowThousand(millions)} million`)
      if (thousands > 0) parts.push(`${belowThousand(thousands)} thousand`)
      if (n > 0) parts.push(belowThousand(n))
      words = parts.join(` `)
    }
  }
  if (fraction !== undefined && fraction !== ``) words += ` point ${digitsWords(fraction)}`
  return words
}

/** How people say an issue number: 1-3 digits as a cardinal, 4 digits as
 * two pairs (`twelve forty-two`, `twelve hundred`, `twelve oh five`, a
 * round thousand as a cardinal), 5+ digits digit by digit. */
export function identifierNumberWords(digits: string): string {
  if (digits.length <= 3) return numberWords(digits)
  if (digits.length > 4) return digitsWords(digits)
  const n = Number(digits)
  if (n % 1000 === 0) return numberWords(digits)
  const high = Math.floor(n / 100)
  const low = n % 100
  if (low === 0) return `${belowHundred(high)} hundred`
  if (low < 10) return `${belowHundred(high)} oh ${ONES[low]}`
  return `${belowHundred(high)} ${belowHundred(low)}`
}

function spell(letters: string): string {
  return letters.toUpperCase().split(``).join(` `)
}

function clip(text: string, maxChars: number): string {
  if (text.length <= maxChars) return text
  const head = text.slice(0, maxChars)
  const sentenceEnd = Math.max(head.lastIndexOf(`. `), head.lastIndexOf(`! `), head.lastIndexOf(`? `))
  const endsWithStop = /[.!?]$/.test(head)
  if (endsWithStop) return head
  if (sentenceEnd > 0) return head.slice(0, sentenceEnd + 1)
  const space = head.lastIndexOf(` `)
  return `${(space > 0 ? head.slice(0, space) : head).trimEnd()}${fixture.copy.ellipsis}`
}

/** What a voice says for a reply's Markdown body (`voice.json` rules). */
export function spokenText(body: string, maxChars: number = fixture.maxChars): string {
  let text = body.replace(/\r\n?/g, `\n`)
  // Fenced code blocks and tables drop.
  text = text.replace(/```[\s\S]*?(```|$)/g, `\n`)
  text = text
    .split(`\n`)
    .filter((line) => !/^\s*\|/.test(line))
    .join(`\n`)
  // Images drop; links keep their text; bare urls become the word `link`.
  text = text.replace(/!\[[^\]]*\]\([^)]*\)/g, ``)
  text = text.replace(/\[([^\]]+)\]\([^)]*\)/g, `$1`)
  text = text.replace(/https?:\/\/[^\s)]+?(?=[.,;:!?]?(\s|$))/g, fixture.copy.link)
  // Line markers: headings, quotes, bullets, numbered items.
  text = text
    .split(`\n`)
    .map((line) =>
      line
        .replace(/^\s{0,3}#{1,6}\s+/, ``)
        .replace(/^\s*>\s?/, ``)
        .replace(/^\s*[-*+]\s+/, ``)
        .replace(/^\s*\d+[.)]\s+/, ``)
    )
    .join(`\n`)
  // Inline code, emphasis, strike.
  text = text.replace(/`([^`]*)`/g, `$1`)
  text = text.replace(/(\*\*|__)(.+?)\1/g, `$2`)
  text = text.replace(/(\*|_)(?=\S)(.+?)(?<=\S)\1/g, `$2`)
  text = text.replace(/~~(.+?)~~/g, `$1`)
  // Mentions keep the local part; issue refs lose the `#`.
  text = text.replace(/@([A-Za-z0-9._%+-]+)@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/g, `$1`)
  text = text.replace(/#([A-Za-z]{2,10}-\d+)/g, `$1`)
  // Identifiers, then acronyms, then numbers.
  text = text.replace(/\b([A-Za-z]{2,10})-(\d+)\b/g, (_, letters: string, digits: string) =>
    `${spell(letters)} ${identifierNumberWords(digits)}`
  )
  text = text.replace(/\b[A-Z]{2,5}\b/g, (word) => spell(word))
  text = text.replace(/(\d+(?:\.\d+)?)\s?%/g, (_, n: string) => `${numberWords(n)} percent`)
  text = text.replace(/\b\d+(?:\.\d+)?\b/g, (n) => numberWords(n))
  text = text.replace(/\s+/g, ` `).trim()
  return clip(text, maxChars)
}

export interface VoiceAgentLike {
  id: string
  name: string
  team: string
}

export interface VoiceAssistantOption {
  id: string | null
  label: string
}

/** The Assistant picker's options: None first, then the agents by label. */
export function assistantOptions(agents: VoiceAgentLike[]): VoiceAssistantOption[] {
  const counts = new Map<string, number>()
  for (const agent of agents) counts.set(agent.name, (counts.get(agent.name) ?? 0) + 1)
  const options = agents
    .map((agent) => ({
      id: agent.id,
      label: (counts.get(agent.name) ?? 0) > 1 ? `${agent.name} · ${agent.team}` : agent.name,
    }))
    .sort((a, b) => a.label.localeCompare(b.label, `en`))
  return [{ id: null, label: fixture.copy.assistantNone }, ...options]
}
