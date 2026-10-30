// NEW-53: exponentialTalk's voice settings and spoken-reply rules. Pure and
// byte-locked by `fixtures/voice.json`; the desktop mirror
// `domain::voice` replays the same cases, the phones mirror `copy`.

import fixture from "../fixtures/voice.json" with { type: "json" }
import screenMemoryFixture from "@exp/domain-contract/fixtures/screen-memory.json" with { type: "json" }
import meetingFixture from "../fixtures/meeting.json" with { type: "json" }

export const VOICE_COPY = fixture.copy
export const VOICE_TEST_IDS = fixture.testIds
export const VOICE_MAX_CHARS = fixture.maxChars
export const VOICE_SHORTCUT_GRAMMAR = fixture.shortcutGrammar
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
  /** S-3: the member spoke this turn (a call runs with the voice agent, or a
   *  push-to-talk / wake turn waits for or plays its reply); a typed
   *  message is never read aloud. */
  voiceTurn: boolean
  /** #NEW-73: the language replies are spoken in (`languages`). */
  language: string
  /** The languages THIS device's speech worker reports (empty without a
   *  usable GPU, #NEW-74). */
  voices: string[]
  /** When the watcher armed; older rows are a backfill. */
  armedAt: string
  /** #NEW-57: a meeting runs on the voice device (nothing is spoken). */
  meeting?: boolean
}

/** Whether a synced message row is read aloud. */
export function shouldSpeak(message: VoiceMessageLike, context: VoiceContext): boolean {
  if (!context.voiceTurn || !context.agentSessionId || !context.isVoiceDevice) return false
  if (context.meeting) return false
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
  wakeWord: boolean
  /** #NEW-57 (meeting.json): the running meeting, null = none. */
  meetingStartedAt?: string | null
}

/** Apply a change; choosing None for the device or the agent clears the
 *  wake word, so picking again never turns the always-on microphone on;
 *  #NEW-57: and stops a running meeting. S-3: a bound key stays bound. */
export function clearOnNone<T extends VoiceSwitches>(before: T, change: Partial<T>): T {
  const after = { ...before, ...change }
  if (after.voiceDeviceId === null || after.agentId === null) {
    after.wakeWord = false
    if (`meetingStartedAt` in after) after.meetingStartedAt = null
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

/** #NEW-75: THE rule of the page's one-line inputs, as you type: each run
 *  of whitespace controls (tab, LF, VT, FF, CR, NEL) becomes one space
 *  (#FEED-160), every other control character (Cc: C0, DEL, C1) is dropped,
 *  then the value is cut to at most `max` UTF-16 code units (zod's count),
 *  whole code points only. Trimming waits for the save. */
export function voiceTextInput(value: string, max: number): string {
  let out = ``
  for (const char of value.replace(/[\t\n\v\f\r\u0085]+/g, ` `).replace(/\p{Cc}/gu, ``)) {
    if (out.length + char.length > max) break
    out += char
  }
  return out
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
  /** #NEW-58: the Screen memory switch (absent or not available = no row). */
  screenMemory?: ScreenMemoryContext
  /** #NEW-57: the Meeting mode switch (absent or not available = no row). */
  meeting?: { available: boolean; on: boolean; who: string }
  /** #NEW-57: Pause listening (shown while a meeting runs or screen memory is on). */
  pause?: { available: boolean; paused: boolean }
  /** S-3: a call runs on this device (Call reads End call). */
  inCall?: boolean
}

/** #NEW-58: the tray's Screen memory switch (device-local, kept). */
export interface ScreenMemoryContext {
  /** This IDE captures here. */
  available: boolean
  /** The switch. */
  on: boolean
  /** A denylisted window is focused, or the pause hotkey holds it. */
  paused: boolean
}

export type TrayItemId = `open` | `call` | `endCall` | `pushToTalk` | `wakeWord` | `meeting` | `screenMemory` | `pause` | `mute` | `settings` | `quit`

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
  if (context.inCall) {
    menu.push({ kind: `action`, id: `endCall`, label: copy.trayEndCall, enabled: true })
  } else if (context.callAvailable && !context.meeting?.on) {
    menu.push({ kind: `action`, id: `call`, label: copy.trayCall.replace(`{agent}`, agent), enabled: ready })
  }
  menu.push({ kind: `separator` })
  // S-3: no key bound = push to talk off = no item; the item opens Settings.
  if (settings.pttShortcut !== ``) {
    menu.push({ kind: `action`, id: `pushToTalk`, label: `${copy.pushToTalk} · ${shortcut}`, enabled: true })
  }
  menu.push(check(`wakeWord`, `${copy.wakeWord} · ${phrase}`, settings.wakeWord))
  const meeting = context.meeting
  if (meeting?.available) {
    const words = meetingFixture.copy.meeting
    menu.push(check(`meeting`, meeting.on ? `${words} · ${meeting.who}` : words, meeting.on))
  }
  const screen = context.screenMemory
  if (screen?.available) {
    const words = screenMemoryFixture.copy
    const label = screen.on && screen.paused ? `${words.tray} · ${words.paused}` : words.tray
    menu.push({ kind: `check`, id: `screenMemory`, label, checked: screen.on, enabled: true })
  }
  if (context.pause?.available) {
    const label = `${meetingFixture.copy.pause} · ${meetingFixture.pause.label}`
    menu.push({ kind: `check`, id: `pause`, label, checked: context.pause.paused, enabled: true })
  }
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
  /** #NEW-57: a meeting runs here, its channels listen, not paused. */
  meetingListening?: boolean
  /** #FEED-163: a meeting runs on this device, paused or not. */
  meetingRunning?: boolean
  muted: boolean
}

/** #NEW-56: the tray icon; `listening` = the always-on microphone dot,
 *  `recording` (#FEED-163) = the red record dot for a meeting's whole run,
 *  null = no tray on this device. */
export function trayIcon(context: TrayIconContext): `app` | `listening` | `recording` | null {
  if (!context.deviceIsVoiceDevice) return null
  if (context.meetingRunning === true) return `recording`
  return (context.wakeListening && !context.muted) || context.meetingListening === true ? `listening` : `app`
}

/** S-3: why the wake word is or is not listening; the voice device's IDE
 *  reads its worker, every other client the device's doctor row `wake`. */
export type WakeState = keyof typeof fixture.pttCopy.wakeDetail

export interface WakeDetailContext {
  wakeWord: boolean
  /** An agent is picked. */
  agent: boolean
  /** The voice device is online. */
  online: boolean
  /** The voice device's name. */
  device: string
  /** What the worker reported; null = nothing yet. */
  state: WakeState | null
}

/** S-3: the Wake word row's trailing detail ×4 (null = none). */
export function wakeDetail(context: WakeDetailContext): string | null {
  const words = fixture.pttCopy.wakeDetail
  if (!context.wakeWord) return null
  if (!context.agent) return words.noAgent
  if (!context.online) return words.offline.replace(`{device}`, context.device)
  return context.state === null ? words.starting : words[context.state].replace(`{device}`, context.device)
}

/** #NEW-97: what the IDE rail's voice button reads besides the settings. */
export interface RailVoiceContext extends TrayContext {
  /** The speech worker reported the wake word's microphone open. */
  wakeListening: boolean
  /** #NEW-57: trayIcon's `meetingListening`. */
  meetingListening?: boolean
  /** A tray host shows the tray (Linux: the StatusNotifierItem registered). */
  trayHosted: boolean
}

export interface RailVoice {
  /** The rail's destructive dot: the wake word holds the microphone open. */
  badge: boolean
  tooltip: string
  /** The tray's rows minus Open, Quit and the separators. */
  menu: TrayItem[]
}

/** #NEW-97: the IDE rail's voice button while no tray host shows the tray;
 *  null = no button. */
export function railVoice(settings: TraySettings, context: RailVoiceContext): RailVoice | null {
  const meeting = context.meeting?.available && context.meeting.on ? context.meeting : null
  if (context.trayHosted || !(clearOnNone(settings, {}).wakeWord || meeting)) return null
  const tray = trayMenu(settings, context)
  if (tray === null) return null
  const phrase = settings.wakePhrase.trim() || wakePhrasePlaceholder(context.agent ?? `…`)
  return {
    badge: trayIcon(context) === `listening` || trayIcon(context) === `recording`,
    tooltip: meeting
      ? `${meetingFixture.copy.meeting} · ${meeting.who}`
      : context.muted
        ? fixture.pttCopy.overlayMuted
        : fixture.pttCopy.railListening.replace(`{phrase}`, phrase),
    menu: tray.filter((item) => item.kind !== `separator` && item.id !== `open` && item.id !== `quit`),
  }
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

const GRAMMAR = fixture.shortcutGrammar
const MODS: readonly string[] = GRAMMAR.mods
const LABELS: Record<string, string> = GRAMMAR.labels

/** The grammar's key (not a modifier): a letter, digit, f1-f24, a named
 *  key or mouse3-mouse9. */
function isShortcutKey(part: string): boolean {
  if (/^[a-z0-9]$/.test(part)) return true
  const fn = /^f([1-9][0-9]?)$/.exec(part)
  if (fn) return Number(fn[1]) >= GRAMMAR.function.min && Number(fn[1]) <= GRAMMAR.function.max
  const mouse = /^mouse([0-9])$/.exec(part)
  if (mouse) return Number(mouse[1]) >= GRAMMAR.mouse.min && Number(mouse[1]) <= GRAMMAR.mouse.max
  return GRAMMAR.named.includes(part)
}

/** A key that may be bound without a modifier (`bareKeys`). */
function isBareKey(part: string): boolean {
  return /^f[0-9]+$/.test(part) || /^mouse[0-9]$/.test(part) || [`insert`, `pause`, `scrolllock`, `printscreen`].includes(part)
}

/** S-3: the canonical text of a binding (`shortcutGrammar`), or null when
 *  the grammar refuses it; empty stays empty (no key = off). */
export function shortcutCanonical(text: string): string | null {
  if (text === ``) return ``
  const parts = text.split(`+`)
  const key = parts.pop() ?? ``
  if (!isShortcutKey(key)) return null
  if (parts.some((part) => !MODS.includes(part)) || new Set(parts).size !== parts.length) return null
  if (parts.length === 0 && !isBareKey(key)) return null
  const value = [...MODS.filter((mod) => parts.includes(mod)), key].join(`+`)
  return value.length > GRAMMAR.maxLength ? null : value
}

function partLabel(part: string): string {
  const mouse = /^mouse([0-9])$/.exec(part)
  if (mouse) return LABELS.mouse.replace(`{n}`, mouse[1])
  return LABELS[part] ?? part.toUpperCase()
}

/** S-3: a binding's label (`Ctrl + Shift + K`); empty reads None. */
export function shortcutLabel(value: string): string {
  if (value === ``) return fixture.copy.shortcutNone
  return value.split(`+`).map(partLabel).join(GRAMMAR.separator)
}

export interface CaptureState {
  status: `recording` | `done` | `cancelled`
  /** The binding (the one before recording until a key records). */
  value: string
  /** The modifiers held now, in canonical order. */
  held: string[]
  error: string | null
}

export type CaptureEvent = { t: `down`; key: string } | { t: `up`; key: string } | { t: `mouse`; button: number }

export function captureStart(value: string): CaptureState {
  return { status: `recording`, value, held: [], error: null }
}

/** S-3: the push-to-talk recorder (`captureCases`). */
export function captureStep(state: CaptureState, event: CaptureEvent): CaptureState {
  if (state.status !== `recording`) return state
  const needsModifier = fixture.pttCopy.shortcutNeedsModifier
  const record = (key: string): CaptureState => {
    const value = shortcutCanonical([...state.held, key].join(`+`))
    return value === null ? { ...state, error: needsModifier } : { status: `done`, value, held: [], error: null }
  }
  if (event.t === `mouse`) return event.button < GRAMMAR.mouse.min ? state : record(`mouse${event.button}`)
  if (MODS.includes(event.key)) {
    const held = event.t === `down` ? [...new Set([...state.held, event.key])] : state.held.filter((mod) => mod !== event.key)
    return { ...state, held: MODS.filter((mod) => held.includes(mod)) }
  }
  if (event.t === `up`) return state
  if (state.held.length === 0 && event.key === `escape`) return { ...state, status: `cancelled`, held: [] }
  if (state.held.length === 0 && (event.key === `backspace` || event.key === `delete`)) {
    return { status: `done`, value: ``, held: [], error: null }
  }
  if (event.key === `escape`) return { ...state, error: needsModifier }
  if (!isShortcutKey(event.key)) return state
  return record(event.key)
}

/** S-3: the held modifiers while recording (`Ctrl + …`; empty = none). */
export function capturePending(state: CaptureState): string {
  if (state.status !== `recording` || state.held.length === 0) return ``
  return `${state.held.map(partLabel).join(GRAMMAR.separator)}${GRAMMAR.separator}…`
}

export type VoiceOs = `linux` | `macos` | `windows`
export type VoiceSession = `x11` | `wayland` | null
export type ShortcutSupport = `global` | `portal` | `focused-only` | `unavailable`

function isMouse(shortcut: string): boolean {
  return /(^|\+)mouse[0-9]$/.test(shortcut)
}

/** #NEW-54: how the voice device holds a push-to-talk shortcut. */
export function shortcutSupport(os: VoiceOs, session: VoiceSession, shortcut: string, portal: boolean): ShortcutSupport {
  if (os === `windows`) return `global`
  if (os === `macos`) return isMouse(shortcut) ? `unavailable` : `global`
  if (session !== `wayland`) return `global`
  if (isMouse(shortcut)) return `unavailable`
  return portal ? `portal` : `focused-only`
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
