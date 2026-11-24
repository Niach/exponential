// #NEW-136: the MODEL FALLBACK of the issue search, its pure half. When the
// phrase grammar (lib/issue-search-phrase.ts) does not recognise a sentence,
// the desktop app and iOS 26 offer `Translate to a search`: ONE short call to
// a model on the user's own device or login (never typing, never our server).
// This file decides when the row shows (`issueSearchWantsTranslation`), builds
// the ONE prompt both callers send (`issueSearchTranslationPrompt`) and turns
// the model's answer into a query the engine accepts
// (`sanitizeIssueSearchTranslation`: the output is never trusted).
//
// Web never calls a model: this is the reference and the drift anchor,
// hand-mirrored on desktop (`domain::issue_search_translation`) and iOS
// (`IssueSearchTranslation.swift`), byte-locked by
// `packages/domain-contract/fixtures/issue-search-translation.json` (its
// `$comment` is the spec; read it before changing a rule).
import fixture from "@exp/domain-contract/fixtures/issue-search-translation.json"
import {
  ISSUE_SEARCH_CREATED_PREFIX,
  ISSUE_SEARCH_CREATOR_PREFIX,
  ISSUE_SEARCH_DAY_MS,
  ISSUE_SEARCH_STATE_PREFIX,
  ISSUE_SEARCH_STATUS_PREFIX,
  ISSUE_SEARCH_UPDATED_PREFIX,
  issueSearchTokens,
  issueSearchWords,
  parseIssueSearchRange,
} from "@/lib/issue-search"
import {
  civilFromDays,
  translateIssueSearchPhrase,
  type IssueSearchPhraseMember,
  type IssueSearchPhraseOptions,
} from "@/lib/issue-search-phrase"

/** The numbers and the copy every caller shares (the fixture's `limits`). */
export const ISSUE_SEARCH_TRANSLATION = fixture.limits
/** The instruction line both models get beside the prompt. */
export const ISSUE_SEARCH_TRANSLATION_SYSTEM = fixture.system

const FILTER_PREFIXES = [`by:`, `is:`, `status:`, `created:`, `updated:`, `similar:`]
/** What a model wraps its answer in: quotes, backticks, emphasis. */
const WRAP = `\`"'*“”„‘’«»`
const LABELS = [`search:`, `query:`]
const STRIP = `.,;:!?"'()`

const codePoints = (text: string) => [...text].length

function nowMs(options?: IssueSearchPhraseOptions): number {
  const now = options?.now ?? Date.now()
  return now instanceof Date ? now.getTime() : now
}

function stripWrap(text: string): string {
  const chars = [...text]
  let start = 0
  let end = chars.length
  const wraps = (char: string) => WRAP.includes(char) || char.trim() === ``
  while (start < end && wraps(chars[start]!)) start += 1
  while (end > start && wraps(chars[end - 1]!)) end -= 1
  return chars.slice(start, end).join(``)
}

/** A token without its leading and trailing punctuation (the grammar's). */
function stripPunctuation(token: string): string {
  const chars = [...token]
  let start = 0
  let end = chars.length
  while (start < end && STRIP.includes(chars[start]!)) start += 1
  while (end > start && STRIP.includes(chars[end - 1]!)) end -= 1
  return chars.slice(start, end).join(``)
}

/** Whether `value` is one word of letters and digits (the engine's words). */
function isWord(value: string): boolean {
  const words = issueSearchWords(value)
  return words.length === 1 && words[0] === value
}

/** One line of the model's answer without its fence, quotes and label. */
function cleanLine(line: string): string {
  const trimmed = line.trim()
  // A code fence line (```` ``` ```` or ```` ```text ````) is not the answer.
  if (trimmed.startsWith("```") && !(trimmed.length > 3 && trimmed.endsWith("`"))) return ``
  const bare = stripWrap(trimmed)
  for (const label of LABELS) {
    if (bare.slice(0, label.length).toLowerCase() === label) return stripWrap(bare.slice(label.length))
  }
  return bare
}

interface Member {
  words: string[]
  local: string
}

function membersOf(options?: IssueSearchPhraseOptions): Member[] {
  return (options?.members ?? []).map((member: IssueSearchPhraseMember) => {
    const email = (member.email ?? ``).toLowerCase()
    const at = email.indexOf(`@`)
    return {
      words: issueSearchWords((member.name ?? ``).toLowerCase()),
      local: at >= 0 ? email.slice(0, at) : email,
    }
  })
}

/**
 * Whether the global search offers `Translate to a search` for `text`: the
 * text has `minWords` or more words, is at most `maxTextLength` long, carries
 * no filter token, and the phrase grammar does not recognise it.
 */
export function issueSearchWantsTranslation(text: string, options?: IssueSearchPhraseOptions): boolean {
  if (codePoints(text.trim()) > ISSUE_SEARCH_TRANSLATION.maxTextLength) return false
  let words = 0
  for (const raw of text.toLowerCase().split(/\s+/u)) {
    const token = stripPunctuation(raw)
    if (FILTER_PREFIXES.some((prefix) => token.startsWith(prefix))) return false
    if (issueSearchWords(token).length > 0) words += 1
  }
  if (words < ISSUE_SEARCH_TRANSLATION.minWords) return false
  return translateIssueSearchPhrase(text, options) === null
}

/**
 * The query a model's answer means, or null. Only tokens the engine accepts
 * survive, in the phrase grammar's order (is:, status:, by:, the dates, the
 * words); null when nothing is left or nothing changed against `text`.
 */
export function sanitizeIssueSearchTranslation(
  output: string,
  text: string,
  options?: IssueSearchPhraseOptions
): string | null {
  const now = nowMs(options)
  const members = membersOf(options)
  const isMember = (word: string) =>
    members.some((member) => member.words.includes(word) || (member.local.length > 0 && member.local === word))

  let line = ``
  for (const raw of output.split(`\n`)) {
    line = cleanLine(raw)
    if (line.length > 0) break
  }

  const states: string[] = []
  const statuses: string[] = []
  const creators: string[] = []
  const dates: string[] = []
  const words: string[] = []
  const add = (list: string[], value: string) => {
    if (!list.includes(value)) list.push(value)
  }
  const plain = (token: string) => {
    const word = stripPunctuation(token)
    if (issueSearchWords(word).length === 0) return
    if (codePoints(word) > ISSUE_SEARCH_TRANSLATION.maxWordLength) return
    if (words.length >= ISSUE_SEARCH_TRANSLATION.maxWords && !words.includes(word)) return
    add(words, word)
  }

  for (const token of issueSearchTokens(line).slice(0, ISSUE_SEARCH_TRANSLATION.maxTokens)) {
    if (token.startsWith(ISSUE_SEARCH_STATE_PREFIX)) {
      const value = token.slice(ISSUE_SEARCH_STATE_PREFIX.length)
      if (value === `open` || value === `closed`) add(states, value)
    } else if (token.startsWith(ISSUE_SEARCH_STATUS_PREFIX)) {
      const value = token.slice(ISSUE_SEARCH_STATUS_PREFIX.length)
      if (isWord(value) && codePoints(value) <= ISSUE_SEARCH_TRANSLATION.maxWordLength) add(statuses, value)
    } else if (token.startsWith(ISSUE_SEARCH_CREATOR_PREFIX)) {
      let value = token.slice(ISSUE_SEARCH_CREATOR_PREFIX.length)
      if (value.startsWith(`@`)) value = value.slice(1)
      // A name that is no member is a plain word, like in the grammar.
      if (isMember(value)) add(creators, value)
      else if (!value.includes(`:`)) plain(value)
    } else if (token.startsWith(ISSUE_SEARCH_CREATED_PREFIX)) {
      const value = token.slice(ISSUE_SEARCH_CREATED_PREFIX.length)
      if (parseIssueSearchRange(value, now)) add(dates, `${ISSUE_SEARCH_CREATED_PREFIX}${value}`)
    } else if (token.startsWith(ISSUE_SEARCH_UPDATED_PREFIX)) {
      const value = token.slice(ISSUE_SEARCH_UPDATED_PREFIX.length)
      if (parseIssueSearchRange(value, now)) add(dates, `${ISSUE_SEARCH_UPDATED_PREFIX}${value}`)
    } else if (!token.includes(`:`)) {
      plain(token)
    }
  }

  const query = [
    ...states.map((state) => `${ISSUE_SEARCH_STATE_PREFIX}${state}`),
    ...statuses.map((status) => `${ISSUE_SEARCH_STATUS_PREFIX}${status}`),
    ...creators.map((creator) => `${ISSUE_SEARCH_CREATOR_PREFIX}${creator}`),
    ...dates,
    ...words,
  ].join(` `)
  if (query.length === 0) return null
  return query === issueSearchTokens(text).join(` `) ? null : query
}

/** The members' FIRST names (lowercased, no repeats, at most `maxNames`):
 *  all the prompt tells a model about the team. Never an email. */
function firstNames(options?: IssueSearchPhraseOptions): string[] {
  const names: string[] = []
  for (const member of membersOf(options)) {
    const first = member.words[0]
    if (first === undefined || codePoints(first) > ISSUE_SEARCH_TRANSLATION.maxWordLength) continue
    if (names.includes(first)) continue
    if (names.length >= ISSUE_SEARCH_TRANSLATION.maxNames) break
    names.push(first)
  }
  return names
}

const pad = (value: number, width: number) => String(value).padStart(width, `0`)

/**
 * THE prompt (desktop and iOS send this exact string): the token syntax, the
 * members' first names, today's UTC date, "one line, the query only", German
 * and English examples, then the sentence (whitespace collapsed, capped).
 */
export function issueSearchTranslationPrompt(text: string, options?: IssueSearchPhraseOptions): string {
  const today = civilFromDays(Math.floor(nowMs(options) / ISSUE_SEARCH_DAY_MS))
  const names = firstNames(options)
  const sentence = [...text.split(/\s+/u).filter((part) => part.length > 0).join(` `)]
    .slice(0, ISSUE_SEARCH_TRANSLATION.maxTextLength)
    .join(``)
  const lines = [
    `Translate the sentence into one issue search query.`,
    `Tokens:`,
    `is:open or is:closed`,
    `status:<word>, one of backlog, progress, review, done, cancelled, duplicate`,
    ...(names.length > 0 ? [`by:<name> for who created the issue, one of ${names.join(`, `)}`] : []),
    `created:<range> or updated:<range>, a range is 7d, 2w, 1m, 1y, today, yesterday, YYYY-MM-DD, YYYY-MM, >=YYYY-MM-DD, <YYYY-MM-DD or YYYY-MM-DD..YYYY-MM-DD`,
    `any other word is a plain search word: keep only the words that name the topic`,
    `Use no other tokens. Today is ${pad(today.year, 4)}-${pad(today.month, 2)}-${pad(today.day, 2)} (UTC).`,
    `Answer with one line, the query only.`,
    `Examples:`,
    `offene Issues zum Editor aus letzter Woche -> is:open created:7d editor`,
    `which crashes did we close since yesterday -> is:closed updated:yesterday crash`,
    `was wurde im letzten Monat zur Abrechnung erledigt -> status:done updated:1m abrechnung`,
    `everything in review about the login page -> status:review login page`,
    ...(names.length > 0 ? [`what did ${names[0]} file about sync this year -> by:${names[0]} created:1y sync`] : []),
    `Sentence: ${sentence}`,
  ]
  return lines.join(`\n`)
}
