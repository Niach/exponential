// #NEW-133: the natural-language PHRASE GRAMMAR of the issue search. A
// sentence (`offene Issues von Lukas zum Editor aus letzter Woche`) becomes
// the structured query the engine already runs (`is:open by:lukas created:7d
// editor`, lib/issue-search.ts). Deterministic, no model, German + English.
// Hand-mirrored on iOS (`IssueSearchPhrase.swift`), Android
// (`IssueSearchPhrase.kt`) and desktop (`domain::issue_search_phrase`),
// byte-locked by `packages/domain-contract/fixtures/issue-search-phrase.json`
// (its `$comment` is the spec; read it before changing a rule).
import { searchSuggestionLabel } from "@exp/domain-contract/search-list"
import {
  ISSUE_SEARCH_DAY_MS,
  daysFromCivil,
  issueSearchWords,
  parseIssueSearchDate,
} from "@/lib/issue-search"

export interface IssueSearchPhraseMember {
  name?: string | null
  email?: string | null
}

export interface IssueSearchPhraseOptions {
  /** The team's members: a name after `von`/`by` that is one of them turns
   *  into `by:`. */
  members?: readonly IssueSearchPhraseMember[]
  /** The clock `this week|month|year` and month names resolve against
   *  (epoch ms or a Date); default = now. */
  now?: number | Date
}

const set = (words: string) => new Set(words.split(` `))

const FILTER_PREFIXES = [`by:`, `is:`, `status:`, `created:`, `updated:`, `similar:`]
const STRIP = `.,;:!?"'()`

const OPEN = set(
  `open offen offene offenen offener offenes unresolved ungelöst ungelöste ungelösten unfinished unerledigt unerledigte unerledigten`
)
const CLOSED = set(`closed geschlossen geschlossene geschlossenen resolved gelöst gelöste gelösten`)
const STATUS_WORDS: ReadonlyArray<[string, Set<string>]> = [
  [`backlog`, set(`backlog`)],
  [
    `done`,
    set(
      `done erledigt erledigte erledigten fertig fertige fertigen completed finished abgeschlossen abgeschlossene abgeschlossenen`
    ),
  ],
  [
    `cancelled`,
    set(`cancelled canceled abgebrochen abgebrochene abgebrochenen storniert stornierte stornierten`),
  ],
  [`duplicate`, set(`duplicate duplicates duplikat duplikate duplikaten`)],
]
/** Two-word statuses: `in progress`, `in arbeit`, `in review`. */
const STATUS_PAIRS: ReadonlyArray<[string, string, string]> = [
  [`in`, `progress`, `progress`],
  [`in`, `arbeit`, `progress`],
  [`in`, `bearbeitung`, `progress`],
  [`in`, `review`, `review`],
  [`im`, `review`, `review`],
]
const CREATOR_LEAD = set(`von vom by from`)
/** A creator lead-in that is dropped even when no member follows. */
const CREATOR_DROP = set(`von vom by`)
const DATE_LEAD = set(`aus seit since from in im during within während innerhalb von vom der dem den des the`)
const MONTH_LEAD = set(`in im from since seit aus during während`)
const LAST = set(`letzter letzten letzte letztem letztes last past previous vergangene vergangenen vergangener vergangenes`)
const THIS = set(`diese dieser diesen diesem dieses this current aktuelle aktuellen aktueller`)
const UNITS: ReadonlyArray<[string, Set<string>]> = [
  [`d`, set(`tag tage tagen day days`)],
  [`w`, set(`woche wochen week weeks`)],
  [`m`, set(`monat monate monaten month months`)],
  [`y`, set(`jahr jahre jahren year years`)],
]
const TODAY = set(`heute today`)
const YESTERDAY = set(`gestern yesterday`)
const FROM = set(`seit since ab after nach from`)
const UNTIL = set(`vor before bis until`)
const MONTHS: readonly string[][] = [
  [`januar`, `january`],
  [`februar`, `february`],
  [`märz`, `march`],
  [`april`],
  [`mai`, `may`],
  [`juni`, `june`],
  [`juli`, `july`],
  [`august`],
  [`september`],
  [`oktober`, `october`],
  [`november`],
  [`dezember`, `december`],
]
const UPDATED = set(
  `updated changed modified edited geändert geänderte geänderten aktualisiert aktualisierte aktualisierten bearbeitet bearbeitete bearbeiteten`
)
const NOUNS = set(`issue issues ticket tickets aufgabe aufgaben task tasks vorgang vorgänge`)
const NOISE = set(
  `created erstellt erstellte erstellten angelegt angelegte angelegten filed opened eröffnet gemeldet gemeldete gemeldeten reported ` +
    `alle all zeige zeig show me mir find finde suche such search list liste the der die das den dem des a an ein eine einen einem einer ` +
    `that which were was wurden wurde sind are is ist hat haben have has been worden und and mit with zum zur zu über about regarding on ` +
    `for für wegen bezüglich in im aus from of to welche welcher bitte please give gib any es gibt there`
)

function unitOf(token: string | undefined): string | null {
  if (token === undefined) return null
  for (const [unit, words] of UNITS) if (words.has(token)) return unit
  return null
}

function monthOf(token: string | undefined): number {
  if (token === undefined) return 0
  return MONTHS.findIndex((names) => names.includes(token)) + 1
}

function isYear(token: string | undefined): token is string {
  return token !== undefined && /^[0-9]{4}$/.test(token)
}

function isCount(token: string | undefined): token is string {
  return token !== undefined && /^[0-9]{1,4}$/.test(token) && Number(token) >= 1
}

const pad = (value: number, width: number) => String(value).padStart(width, `0`)

/** The UTC civil date of a day number (Howard Hinnant's civil_from_days). */
function civilFromDays(days: number): { year: number; month: number; day: number } {
  const z = days + 719_468
  const era = Math.floor(z / 146_097)
  const doe = z - era * 146_097
  const yoe = Math.floor((doe - Math.floor(doe / 1460) + Math.floor(doe / 36_524) - Math.floor(doe / 146_096)) / 365)
  const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100))
  const mp = Math.floor((5 * doy + 2) / 153)
  const month = mp < 10 ? mp + 3 : mp - 9
  return {
    year: yoe + era * 400 + (month <= 2 ? 1 : 0),
    month,
    day: doy - Math.floor((153 * mp + 2) / 5) + 1,
  }
}

/** A date phrase starting at `tokens[i]`: its `created:`/`updated:` value
 *  and how many tokens it spans, or null. */
function dateAt(tokens: readonly string[], i: number, today: number): { value: string; length: number } | null {
  const t = tokens[i]!
  const next = tokens[i + 1]
  if (TODAY.has(t)) return { value: `today`, length: 1 }
  if (YESTERDAY.has(t)) return { value: `yesterday`, length: 1 }
  if (LAST.has(t)) {
    const unit = unitOf(next)
    if (unit) return { value: unit === `w` ? `7d` : `1${unit}`, length: 2 }
    const counted = unitOf(tokens[i + 2])
    if (isCount(next) && counted) return { value: `${Number(next)}${counted}`, length: 3 }
    return null
  }
  if (THIS.has(t)) {
    const unit = unitOf(next)
    const now = civilFromDays(today)
    if (unit === `d`) return { value: `today`, length: 2 }
    if (unit === `w`) {
      // Monday of the current UTC week (1970-01-01 was a Thursday).
      const monday = civilFromDays(today - ((((today + 3) % 7) + 7) % 7))
      return { value: `>=${pad(monday.year, 4)}-${pad(monday.month, 2)}-${pad(monday.day, 2)}`, length: 2 }
    }
    if (unit === `m`) return { value: `${pad(now.year, 4)}-${pad(now.month, 2)}`, length: 2 }
    if (unit === `y`) return { value: `>=${pad(now.year, 4)}-01-01`, length: 2 }
    return null
  }
  if (FROM.has(t) && next !== undefined && parseIssueSearchDate(next)) return { value: `>=${next}`, length: 2 }
  if (UNTIL.has(t) && next !== undefined && parseIssueSearchDate(next)) return { value: `<${next}`, length: 2 }
  if (parseIssueSearchDate(t)) return { value: t, length: 1 }
  // A month name counts only after a lead-in (`im Mai`, `in may`) or before
  // a year (`mai 2026`): `may` and `march` are words too.
  const month = monthOf(next)
  if (MONTH_LEAD.has(t) && month > 0) {
    if (isYear(tokens[i + 2])) return { value: `${tokens[i + 2]}-${pad(month, 2)}`, length: 3 }
    const now = civilFromDays(today)
    return { value: `${pad(month > now.month ? now.year - 1 : now.year, 4)}-${pad(month, 2)}`, length: 2 }
  }
  const bare = monthOf(t)
  if (bare > 0 && isYear(next)) return { value: `${next}-${pad(bare, 2)}`, length: 2 }
  return null
}

/**
 * The structured query a sentence means, or null when the grammar does not
 * recognise one (plain words, a query that already carries a filter token,
 * or nothing that would change).
 */
export function translateIssueSearchPhrase(text: string, options?: IssueSearchPhraseOptions): string | null {
  const nowOption = options?.now ?? Date.now()
  const nowMs = nowOption instanceof Date ? nowOption.getTime() : nowOption
  const today = Math.floor(nowMs / ISSUE_SEARCH_DAY_MS)

  const tokens: string[] = []
  for (const raw of text.toLowerCase().split(/\s+/u)) {
    let start = 0
    let end = raw.length
    while (start < end && STRIP.includes(raw[start]!)) start += 1
    while (end > start && STRIP.includes(raw[end - 1]!)) end -= 1
    const token = raw.slice(start, end)
    if (token.length === 0) continue
    if (FILTER_PREFIXES.some((prefix) => token.startsWith(prefix))) return null
    tokens.push(token)
  }

  // Each member's name words and email local part, lowercased.
  const members = (options?.members ?? []).map((member) => {
    const email = (member.email ?? ``).toLowerCase()
    const at = email.indexOf(`@`)
    return {
      words: issueSearchWords((member.name ?? ``).toLowerCase()),
      local: at >= 0 ? email.slice(0, at) : email,
    }
  })
  const memberOf = (token: string) =>
    members.filter((member) => member.words.includes(token) || (member.local.length > 0 && member.local === token))

  const states: string[] = []
  const statuses: string[] = []
  const creators: string[] = []
  const dates: string[] = []
  const words: string[] = []
  let updated = false
  let noun = false
  let strong = 0
  let weak = 0
  const add = (list: string[], value: string) => {
    if (!list.includes(value)) list.push(value)
  }

  let i = 0
  while (i < tokens.length) {
    const t = tokens[i]!
    const next = tokens[i + 1]

    // 1. A date phrase, behind up to two lead-in words (`aus der letzten Woche`).
    let lead = 0
    let date = dateAt(tokens, i, today)
    while (!date && lead < 2 && i + lead < tokens.length && DATE_LEAD.has(tokens[i + lead]!)) {
      lead += 1
      if (i + lead < tokens.length) date = dateAt(tokens, i + lead, today)
    }
    if (date) {
      add(dates, date.value)
      strong += 1
      i += lead + date.length
      continue
    }

    // 2. A two-word status, then the one-word states and statuses.
    const pair = STATUS_PAIRS.find(([first, second]) => first === t && second === next)
    if (pair) {
      add(statuses, pair[2])
      weak += 1
      i += 2
      continue
    }
    if (OPEN.has(t)) {
      add(states, `open`)
      weak += 1
      i += 1
      continue
    }
    if (CLOSED.has(t)) {
      add(states, `closed`)
      weak += 1
      i += 1
      continue
    }
    const status = STATUS_WORDS.find(([, names]) => names.has(t))
    if (status) {
      add(statuses, status[0])
      weak += 1
      i += 1
      continue
    }

    // 3. `von|by|from <member>`: every following word that is a name word of
    //    the same member joins (`von Lukas Mai`).
    if (CREATOR_LEAD.has(t)) {
      let matched = next === undefined ? [] : memberOf(next)
      if (matched.length > 0) {
        let j = i + 1
        while (j < tokens.length) {
          const narrowed = matched.filter((member) => member.words.includes(tokens[j]!) || member.local === tokens[j])
          if (narrowed.length === 0) break
          matched = narrowed
          add(creators, tokens[j]!)
          j += 1
        }
        strong += 1
        i = j
        continue
      }
      if (CREATOR_DROP.has(t)) {
        i += 1
        continue
      }
    }

    // 4. The date field, the nouns and the filler words.
    if (UPDATED.has(t)) {
      updated = true
      i += 1
      continue
    }
    if (NOUNS.has(t)) {
      noun = true
      i += 1
      continue
    }
    if (NOISE.has(t)) {
      i += 1
      continue
    }
    words.push(t)
    i += 1
  }

  // A date or a member is a sentence on its own; a state or status word
  // needs the noun beside it (`open issues`, not `open file dialog`).
  if (strong === 0 && !(weak > 0 && noun)) return null
  const field = updated ? `updated:` : `created:`
  const query = [
    ...states.map((state) => `is:${state}`),
    ...statuses.map((status) => `status:${status}`),
    ...creators.map((creator) => `by:${creator}`),
    ...dates.map((date) => `${field}${date}`),
    ...words,
  ].join(` `)
  return query === tokens.join(` `) ? null : query
}

/** The global search's suggestion row (search-list.json SUGGESTION): the
 *  query a recognised sentence means and the row's name, or null. */
export interface IssueSearchSuggestion {
  query: string
  label: string
}

export function issueSearchSuggestion(text: string, options?: IssueSearchPhraseOptions): IssueSearchSuggestion | null {
  const query = translateIssueSearchPhrase(text, options)
  return query === null ? null : { query, label: searchSuggestionLabel(query) }
}
