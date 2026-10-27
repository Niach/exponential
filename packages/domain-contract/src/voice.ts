// NEW-53: exponentialTalk's voice settings and spoken-reply rules. Pure and
// byte-locked by `fixtures/voice.json`; the desktop mirror
// `domain::voice` replays the same cases, the phones mirror `copy`.

import fixture from "../fixtures/voice.json" with { type: "json" }

export const VOICE_COPY = fixture.copy
export const VOICE_TEST_IDS = fixture.testIds
export const VOICE_MAX_CHARS = fixture.maxChars
export const VOICE_SHORTCUTS = fixture.shortcuts
export const VOICE_DEFAULTS = fixture.defaults
export const VOICE_LANGUAGES = fixture.languages
export const VOICE_LIMITS = fixture.limits

/** #NEW-74: the words a spoken reply uses per language (`spokenWords`). */
export interface VoiceSpokenWords {
  link: string
  point: string
  percent: string
}

const SPOKEN_WORDS: Record<string, VoiceSpokenWords> = fixture.spokenWords

/** The spoken words of a language; an unknown one reads as English. */
export function spokenWords(language: string): VoiceSpokenWords {
  return SPOKEN_WORDS[language] ?? SPOKEN_WORDS.en
}

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
  /** The member's conversation session with the voice agent. */
  agentSessionId: string | null
  /** THIS device is the member's voice device. */
  isVoiceDevice: boolean
  speakReplies: boolean
  /** #NEW-73: the language replies are spoken in (`languages`). */
  language: string
  /** The languages THIS device's speech worker reports (empty without a
   *  usable GPU, #NEW-74). */
  voices: string[]
  /** When the watcher armed; older rows are a backfill. */
  armedAt: string
}

/** Whether a synced message row is read aloud. */
export function shouldSpeak(message: VoiceMessageLike, context: VoiceContext): boolean {
  if (!context.speakReplies || !context.agentSessionId || !context.isVoiceDevice) return false
  if (!context.voices.includes(context.language)) return false
  if (message.authorKind !== `agent`) return false
  if (message.card) return false
  if (!message.body || message.body.trim() === ``) return false
  if (message.authorSessionId) return false
  if (message.sessionId !== context.agentSessionId) return false
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

const ONES_DE = [
  `null`, `eins`, `zwei`, `drei`, `vier`, `fünf`, `sechs`, `sieben`, `acht`, `neun`,
  `zehn`, `elf`, `zwölf`, `dreizehn`, `vierzehn`, `fünfzehn`, `sechzehn`,
  `siebzehn`, `achtzehn`, `neunzehn`,
]
const TENS_DE = [``, ``, `zwanzig`, `dreißig`, `vierzig`, `fünfzig`, `sechzig`, `siebzig`, `achtzig`, `neunzig`]

/** A German digit in front of `und`, `hundert` or `tausend` (`ein`, not `eins`). */
function unitDe(n: number): string {
  return n === 1 ? `ein` : ONES_DE[n]
}

function belowHundredDe(n: number): string {
  if (n < 20) return ONES_DE[n]
  const tens = TENS_DE[Math.floor(n / 10)]
  return n % 10 === 0 ? tens : `${unitDe(n % 10)}und${tens}`
}

/** One written compound: `einhunderteins`, `siebenhundertdreiundachtzig`. */
function belowThousandDe(n: number): string {
  const hundreds = Math.floor(n / 100)
  const rest = n % 100
  let words = hundreds > 0 ? `${unitDe(hundreds)}hundert` : ``
  if (rest > 0 || hundreds === 0) words += belowHundredDe(rest)
  return words
}

function digitsWords(digits: string, language: string = `en`): string {
  const ones = language === `de` ? ONES_DE : ONES
  return digits.split(``).map((d) => ones[Number(d)]).join(` `)
}

/** German cardinal words for an integer below a billion. */
function wholeWordsDe(whole: number): string {
  if (whole === 0) return ONES_DE[0]
  const millions = Math.floor(whole / 1_000_000)
  const thousands = Math.floor((whole % 1_000_000) / 1_000)
  const rest = whole % 1_000
  const parts: string[] = []
  if (millions === 1) parts.push(`eine Million`)
  else if (millions > 1) parts.push(`${belowThousandDe(millions).replace(/eins$/, `eine`)} Millionen`)
  let tail = ``
  if (thousands > 0) tail += `${belowThousandDe(thousands).replace(/eins$/, `ein`)}tausend`
  if (rest > 0) tail += belowThousandDe(rest)
  if (tail !== ``) parts.push(tail)
  return parts.join(` `)
}

/** Cardinal words for a number written with digits, in the language
 * (`en` or `de`; an optional decimal part reads `spokenWords.point` and
 * then digit by digit). */
export function numberWords(text: string, language: string = `en`): string {
  const [whole, fraction] = text.split(`.`)
  const german = language === `de`
  let words: string
  if (whole.length > 9) {
    words = digitsWords(whole, language)
  } else if (german) {
    words = wholeWordsDe(Number(whole))
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
  if (fraction !== undefined && fraction !== ``) {
    words += ` ${spokenWords(language).point} ${digitsWords(fraction, language)}`
  }
  return words
}

/** How people say an issue number: 1-3 digits as a cardinal, 4 digits as
 * two pairs (`twelve forty-two`, `twelve hundred`, `twelve oh five`, a
 * round thousand as a cardinal; German `zwölf zweiundvierzig`,
 * `zwölfhundert`, `zwölf null fünf`), 5+ digits digit by digit. */
export function identifierNumberWords(digits: string, language: string = `en`): string {
  if (digits.length <= 3) return numberWords(digits, language)
  if (digits.length > 4) return digitsWords(digits, language)
  const n = Number(digits)
  if (n % 1000 === 0) return numberWords(digits, language)
  const high = Math.floor(n / 100)
  const low = n % 100
  if (language === `de`) {
    if (low === 0) return `${belowHundredDe(high)}hundert`
    if (low < 10) return `${belowHundredDe(high)} null ${ONES_DE[low]}`
    return `${belowHundredDe(high)} ${belowHundredDe(low)}`
  }
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

/** What a voice says for a reply's Markdown body in the language
 * (`voice.json` rules; an unknown language reads as English). */
export function spokenText(body: string, maxChars: number = fixture.maxChars, language: string = `en`): string {
  const words = spokenWords(language)
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
  text = text.replace(/https?:\/\/[^\s)]+?(?=[.,;:!?]?(\s|$))/g, words.link)
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
    `${spell(letters)} ${identifierNumberWords(digits, language)}`
  )
  text = text.replace(/\b[A-Z]{2,5}\b/g, (word) => spell(word))
  text = text.replace(/(\d+(?:\.\d+)?)\s?%/g, (_, n: string) => `${numberWords(n, language)} ${words.percent}`)
  text = text.replace(/\b\d+(?:\.\d+)?\b/g, (n) => numberWords(n, language))
  text = text.replace(/\s+/g, ` `).trim()
  return clip(text, maxChars)
}

export interface VoiceAgentLike {
  id: string
  name: string
  team: string
}

export interface VoiceOption {
  id: string | null
  label: string
}

/** The Agent picker's options: None first, then the agents by label. */
export function agentOptions(agents: VoiceAgentLike[]): VoiceOption[] {
  const counts = new Map<string, number>()
  for (const agent of agents) counts.set(agent.name, (counts.get(agent.name) ?? 0) + 1)
  const options = agents
    .map((agent) => ({
      id: agent.id,
      label: (counts.get(agent.name) ?? 0) > 1 ? `${agent.name} · ${agent.team}` : agent.name,
    }))
    .sort((a, b) => a.label.localeCompare(b.label, `en`))
  return [{ id: null, label: fixture.copy.agentNone }, ...options]
}

export interface VoiceDeviceLike {
  deviceId: string
  name: string
  /** Contract `deviceKind`; only `desktop` can be a voice device. */
  kind: string
}

/** The Voice device picker's options: None first, then the member's
 *  desktops by name (phones and servers are never a voice device). */
export function voiceDeviceOptions(devices: VoiceDeviceLike[]): VoiceOption[] {
  const options = devices
    .filter((device) => device.kind === `desktop`)
    .map((device) => ({ id: device.deviceId, label: device.name }))
    .sort((a, b) => a.label.localeCompare(b.label, `en`))
  return [{ id: null, label: fixture.copy.voiceDeviceNone }, ...options]
}

export interface VoiceSwitches {
  voiceDeviceId: string | null
  agentId: string | null
  speakReplies: boolean
  pushToTalk: boolean
  wakeWord: boolean
}

/** Apply a change; choosing None for the device or the agent clears every
 *  switch, so picking again never turns listening or speaking on. */
export function clearOnNone<T extends VoiceSwitches>(before: T, change: Partial<T>): T {
  const after = { ...before, ...change }
  if (after.voiceDeviceId === null || after.agentId === null) {
    after.speakReplies = false
    after.pushToTalk = false
    after.wakeWord = false
  }
  return after
}

/** The tray group's header for the voice device. */
export function trayHeader(deviceName: string): string {
  return fixture.copy.trayHeader.replace(`{device}`, deviceName)
}

/** The wake phrase's placeholder (and its value when empty). */
export function wakePhrasePlaceholder(agentName: string): string {
  return fixture.copy.wakePhrasePlaceholder.replace(`{agent}`, agentName)
}

/** #NEW-56: a stored wake phrase needs `limits.wakePhraseMinLetters` letters
 *  (letters only), so a short word can't fire on every segment; empty =
 *  the placeholder, valid. The refusal's text, else null. */
export function wakePhraseError(text: string): string | null {
  const trimmed = text.trim()
  if (trimmed === ``) return null
  const letters = [...trimmed].filter((char) => /\p{L}/u.test(char)).length
  return letters < fixture.limits.wakePhraseMinLetters ? fixture.copy.wakePhraseTooShort : null
}

/** #NEW-73: the Language row's trailing detail; every language speaks on
 *  the voice device's GPU only (#NEW-74). Null without a voice device. */
export function languageNote(_language: string, deviceName: string | null): string | null {
  if (deviceName === null) return null
  return fixture.copy.languageNote.replace(`{device}`, deviceName)
}

/** #NEW-73: what the Language row and its menu show for a language. */
export function languageLabel(language: string, deviceName: string | null): string {
  const label = fixture.languages.find((entry) => entry.value === language)?.label ?? language
  const note = languageNote(language, deviceName)
  return note === null ? label : `${label} · ${note}`
}

export interface TrayContext {
  /** The voice agent's name (null = no agent). */
  agent: string | null
  /** Mute microphone (IDE-local, never stored). */
  muted: boolean
  /** A call can start (V4; false until then). */
  callAvailable: boolean
  /** The microphone listens (V3; false until then): Mute shows. */
  micAvailable: boolean
  /** THIS device is the member's voice device. */
  deviceIsVoiceDevice: boolean
  /** #NEW-54: the push-to-talk shortcut could not be grabbed. */
  shortcutUnavailable?: boolean
  /** #NEW-54: the portal's trigger description (Wayland), else null. */
  shortcutLabel?: string | null
}

export type TrayItemId = `open` | `call` | `speakReplies` | `pushToTalk` | `wakeWord` | `mute` | `settings` | `quit`

export type TrayItem =
  | { kind: `separator` }
  | { kind: `action`; id: TrayItemId; label: string; enabled: boolean }
  | { kind: `check`; id: TrayItemId; label: string; checked: boolean; enabled: boolean }

export interface TraySettings extends VoiceSwitches {
  pttShortcut: string
  wakePhrase: string
}

/** #NEW-68: the tray's OS-native menu; null = no tray on this device. */
export function trayMenu(settings: TraySettings, context: TrayContext): TrayItem[] | null {
  if (!context.deviceIsVoiceDevice) return null
  const copy = fixture.copy
  const agent = context.agent ?? `…`
  const ready = settings.voiceDeviceId !== null && settings.agentId !== null
  const label = context.shortcutLabel ?? shortcutLabel(settings.pttShortcut)
  const shortcut = context.shortcutUnavailable ? fixture.pttCopy.shortcutUnavailable.replace(`{shortcut}`, label) : label
  const phrase = settings.wakePhrase.trim() || wakePhrasePlaceholder(agent)
  const check = (id: TrayItemId, label: string, on: boolean): TrayItem => ({
    kind: `check`,
    id,
    label,
    checked: on && ready,
    enabled: ready,
  })
  const menu: TrayItem[] = [{ kind: `action`, id: `open`, label: copy.trayOpen, enabled: true }]
  if (context.callAvailable) {
    menu.push({ kind: `action`, id: `call`, label: copy.trayCall.replace(`{agent}`, agent), enabled: ready })
  }
  menu.push(
    { kind: `separator` },
    check(`speakReplies`, copy.speakReplies, settings.speakReplies),
    check(`pushToTalk`, `${copy.pushToTalk} · ${shortcut}`, settings.pushToTalk),
    check(`wakeWord`, `${copy.wakeWord} · ${phrase}`, settings.wakeWord)
  )
  if (context.micAvailable) {
    menu.push({ kind: `check`, id: `mute`, label: copy.trayMute, checked: context.muted, enabled: true })
  }
  menu.push(
    { kind: `separator` },
    { kind: `action`, id: `settings`, label: copy.traySettings, enabled: true },
    { kind: `action`, id: `quit`, label: copy.trayQuit, enabled: true }
  )
  return menu
}

/** #NEW-56: what the tray icon's state reads. */
export interface TrayIconContext {
  deviceIsVoiceDevice: boolean
  /** The speech worker reported the wake word's microphone open. */
  wakeListening: boolean
  muted: boolean
}

/** #NEW-56: the tray icon; `listening` = the always-on microphone dot,
 *  null = no tray on this device. */
export function trayIcon(context: TrayIconContext): `app` | `listening` | null {
  if (!context.deviceIsVoiceDevice) return null
  return context.wakeListening && !context.muted ? `listening` : `app`
}

/** #NEW-86: the IDE's Wake word row detail on the voice device itself
 *  (desktop only): Microphone muted, Listening while the wake word holds the
 *  microphone, else null. */
export function wakeDetail(context: { here: boolean; wakeWord: boolean; wakeListening: boolean; muted: boolean }): string | null {
  if (!context.here || !context.wakeWord) return null
  if (context.muted) return fixture.pttCopy.overlayMuted
  return context.wakeListening ? fixture.pttCopy.overlayListening : null
}

const GB = 1_000_000_000
const MB = 1_000_000

/** One size for the download detail: MB below 1 GB, else GB with one
 *  decimal; `down` rounds down (received never overstates progress). */
function downloadSize(bytes: number, down: boolean): string {
  const round = down ? Math.floor : Math.round
  if (bytes < GB) return `${round(bytes / MB)} MB`
  const tenths = round(bytes / (GB / 10))
  return `${Math.floor(tenths / 10)}.${tenths % 10} GB`
}

/** The Speak replies row's detail while the voice downloads; a total of 0
 *  (unknown) reads `copy.downloading`. */
export function downloadProgress(received: number, total: number): string {
  if (total <= 0) return fixture.copy.downloading
  return fixture.copy.downloadingProgress
    .replace(`{received}`, downloadSize(received, true))
    .replace(`{total}`, downloadSize(total, false))
}

/** #NEW-54: a shortcut's label (`shortcuts`), the value itself when unknown. */
export function shortcutLabel(value: string): string {
  return fixture.shortcuts.find((entry) => entry.value === value)?.label ?? value
}

export type VoiceOs = `linux` | `macos` | `windows`
export type VoiceSession = `x11` | `wayland` | null
export type ShortcutSupport = `global` | `portal` | `focused-only` | `unavailable`

function isMouse(shortcut: string): boolean {
  return shortcut.startsWith(`mouse`)
}

/** #NEW-54: how the voice device holds a push-to-talk shortcut. */
export function shortcutSupport(os: VoiceOs, session: VoiceSession, shortcut: string, portal: boolean): ShortcutSupport {
  if (os === `windows`) return `global`
  if (os === `macos`) return isMouse(shortcut) ? `unavailable` : `global`
  if (session !== `wayland`) return `global`
  if (isMouse(shortcut)) return `unavailable`
  return portal ? `portal` : `focused-only`
}

/** #NEW-54: the Shortcut picker's values on the voice device. */
export function shortcutOptions(os: VoiceOs, session: VoiceSession): string[] {
  return fixture.shortcuts
    .map((entry) => entry.value)
    .filter((value) => shortcutSupport(os, session, value, true) !== `unavailable`)
}

export type ShortcutReason = keyof typeof fixture.pttCopy.shortcutReasons

/** #NEW-54: why the shortcut can't be held, under the Shortcut row. */
export function shortcutReason(reason: ShortcutReason, shortcut: string): string {
  return fixture.pttCopy.shortcutReasons[reason].replace(`{shortcut}`, shortcutLabel(shortcut))
}

/** #NEW-54: what a heard utterance sends: trimmed, null when blank. */
export function utteranceText(heard: string): string | null {
  const text = heard.trim()
  return text === `` ? null : text
}

export type UtteranceInput = { sessionId: string; text: string } | { teamId: string; agentId: string; text: string }

/** #NEW-54: the `sessions.send` of an utterance: into the conversation with
 *  the agent, or to the agent while there is none yet. */
export function utteranceInput(
  heard: string,
  target: { teamId: string; agentId: string | null; conversationId: string | null }
): UtteranceInput | null {
  const text = utteranceText(heard)
  if (text === null || target.agentId === null) return null
  if (target.conversationId !== null) return { sessionId: target.conversationId, text }
  return { teamId: target.teamId, agentId: target.agentId, text }
}

/** #NEW-91: the sentences of a spoken text (split after `.` `!` `?` + a space). */
function spokenSentences(spoken: string): string[] {
  const out: string[] = []
  let start = 0
  for (let index = 0; index < spoken.length; index++) {
    if (`.!?`.includes(spoken[index]) && spoken[index + 1] === ` `) {
      out.push(spoken.slice(start, index + 1).trim())
      start = index + 1
    }
  }
  out.push(spoken.slice(start).trim())
  return out.filter((sentence) => sentence !== ``)
}

/** #NEW-91: a sentence's first clause and the rest, or null (see `speechChunks`). */
function firstClause(sentence: string): [string, string] | null {
  for (let index = 0; index < sentence.length; index++) {
    const ch = sentence[index]
    let cut: [number, number] | null = null
    if (`,;:`.includes(ch) && /^\s/.test(sentence.slice(index + 1))) cut = [index + 1, index + 1]
    else if (ch === `-` && index > 0 && sentence[index - 1] === ` ` && sentence[index + 1] === ` `) cut = [index - 1, index + 1]
    if (cut === null) continue
    const front = sentence.slice(0, cut[0]).trim()
    const rest = sentence.slice(cut[1]).trim()
    const words = front.split(/\s+/).filter((word) => word !== ``).length
    return words >= 2 && words <= 8 && rest !== `` ? [front, rest] : null
  }
  return null
}

/** #NEW-91: the synthesis chunks of a spoken reply on the voice device (`speechChunkCases`). */
export function speechChunks(spoken: string, streaming: boolean): string[] {
  const parts = spokenSentences(spoken)
  const clause = !streaming && parts.length > 0 ? firstClause(parts[0]) : null
  if (clause !== null) parts.splice(0, 1, ...clause)
  return parts
}

/** The first sentence of a spoken text (the Dock banner's title). */
export function firstSentence(spoken: string): string {
  const match = /[.!?](?= )/.exec(spoken)
  return match === null ? spoken : spoken.slice(0, match.index + 1)
}

export type PttPhase = `hidden` | `listening` | `heard` | `thinking` | `speaking` | `spoken` | `reply` | `muted` | `notSent` | `failed`
export type ListenFailure = keyof typeof fixture.pttCopy.listenFailed

export interface PttState {
  phase: PttPhase
  line: string
  /** listenFailed's reason. */
  reason?: ListenFailure
}

export type PttEvent =
  | { t: `keyDown`; muted: boolean }
  | { t: `keyUp` }
  | { t: `partial`; text: string }
  | { t: `heard`; text: string }
  | { t: `empty` }
  | { t: `listenFailed`; reason: ListenFailure }
  | { t: `sent` }
  | { t: `sendFailed` }
  | { t: `replied`; body: string; speaking: boolean }
  | { t: `speakFinished` }
  | { t: `cancel` }
  | { t: `hide` }

export const PTT_HIDDEN: PttState = { phase: `hidden`, line: `` }

const quote = (text: string) => `“${text}”`

/** #NEW-54: one key or worker event folded into the push-to-talk state. */
export function pttStep(state: PttState, event: PttEvent): PttState {
  switch (event.t) {
    case `keyDown`:
      return event.muted ? { phase: `muted`, line: fixture.pttCopy.overlayMutedLine } : { phase: `listening`, line: `` }
    case `keyUp`:
      return state
    case `partial`:
      return state.phase === `listening` ? { phase: `listening`, line: quote(event.text.trim()) } : state
    case `heard`: {
      if (state.phase !== `listening`) return state
      const text = utteranceText(event.text)
      return text === null ? PTT_HIDDEN : { phase: `heard`, line: quote(text) }
    }
    case `empty`:
      return state.phase === `listening` ? PTT_HIDDEN : state
    case `listenFailed`:
      return state.phase === `listening` ? { phase: `failed`, line: ``, reason: event.reason } : state
    case `sent`:
      return state.phase === `heard` ? { phase: `thinking`, line: state.line } : state
    case `sendFailed`:
      return state.phase === `heard` ? { phase: `notSent`, line: fixture.pttCopy.overlayNotSentLine } : state
    case `replied`: {
      if (state.phase !== `thinking`) return state
      const line = firstSentence(spokenText(event.body, fixture.maxChars))
      return { phase: event.speaking ? `speaking` : `reply`, line }
    }
    case `speakFinished`:
      return state.phase === `speaking` ? { phase: `spoken`, line: state.line } : state
    case `cancel`:
    case `hide`:
      return PTT_HIDDEN
  }
}

export interface PttOverlay {
  activity: `idle` | `listening` | `thinking` | `speaking` | `muted`
  state: string
  line: string
  pill: string
  primary: boolean
  hideAfterMs: number | null
}

const HIDE_AFTER: Record<string, number> = fixture.overlayHideAfterMs

/** #NEW-54: the capsule for a push-to-talk state; null = hidden. */
export function pttOverlay(state: PttState, agent: string | null): PttOverlay | null {
  const copy = fixture.copy
  const ptt = fixture.pttCopy
  const pill = agent ?? `…`
  const hideAfterMs = HIDE_AFTER[state.phase] ?? null
  const view = (activity: PttOverlay[`activity`], label: string, overrides: Partial<PttOverlay> = {}): PttOverlay => ({
    activity,
    state: label,
    line: state.line,
    pill,
    primary: false,
    hideAfterMs,
    ...overrides,
  })
  switch (state.phase) {
    case `hidden`:
      return null
    case `listening`:
      return view(`listening`, ptt.overlayListening)
    case `heard`:
      return view(`thinking`, ptt.overlayHeard)
    case `thinking`:
      return view(`thinking`, ptt.overlayThinking)
    case `speaking`:
      return view(`speaking`, copy.speaking, { pill: copy.stop, primary: true })
    case `spoken`:
    case `reply`:
      return view(`idle`, ptt.overlayReplied)
    case `muted`:
      return view(`muted`, ptt.overlayMuted, { pill: ptt.overlayUnmute })
    case `notSent`:
      return view(`idle`, ptt.overlayNotSent)
    case `failed`:
      return view(`idle`, ptt.listenFailed[state.reason ?? `failed`])
  }
}
