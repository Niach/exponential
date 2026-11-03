// eApps R6 (VAPP-222, docs/spec/apps.md §3.3/§3.4): a kind's state machine on
// ONE enum field. `records.<kind>.transitions`:
//
//   { "field": "status", "initial"?: ["requested"],
//     "from": { "requested": ["approved", "rejected"], "approved": ["paid"] },
//     "who"?: { "requested->approved": "role:approver", "paid": "owners" } }
//
// A write that moves the field along an undeclared edge is refused
// (BAD_REQUEST naming it), and one by someone the edge's `who` excludes
// (FORBIDDEN). `who` names an edge (`a->b`) or a target (`b`: every edge into
// it; the edge wins); its level is `member` (the default), `author` (the
// record's author or a team owner), `owners` or `role:<field>` (the member
// a `member` ref of the record names BEFORE the write, or a team owner).
// Pure, mirrored line by line in the Rust core (`host::record_transitions`);
// fixtures/record-transitions.json is the spec. The Exponential server
// enforces it (the records service); a client host offers only the legal
// next states (`transitionsFrom`; a records row of the kind carries them as
// `transitions`).

import type { PackageIssue } from "./package"
import { recordAuthorOf, type RecordOwnership } from "./record-policy"

export interface RecordTransitions {
  field: string
  initial?: string[]
  from: Record<string, string[]>
  who?: Record<string, string>
}

/** Who moves a record: the member id and their team role (`owner` or not). */
export interface TransitionViewer {
  id: string | null
  role: string | null
}

export interface TransitionRefusal {
  code: `BAD_REQUEST` | `FORBIDDEN`
  message: string
}

/** The key a records row of a kind with transitions carries its legal next states in. */
export const RECORD_TRANSITIONS_ROW_KEY = `transitions`
export const TRANSITION_LEVELS: readonly string[] = [`member`, `author`, `owners`]
const KEYS: readonly string[] = [`field`, `initial`, `from`, `who`]
const ARROW = `->`

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)
const own = (o: Record<string, unknown>, k: string) => Object.prototype.hasOwnProperty.call(o, k)
const isStringList = (v: unknown): v is string[] => Array.isArray(v) && v.every((s) => typeof s === `string`)

/** A kind's transitions when well-formed enough to apply (`field` a string,
 *  `from` an object of lists); else null. Bad entries are dropped. */
export function kindTransitions(kind: unknown): RecordTransitions | null {
  const t = isObject(kind) ? kind.transitions : undefined
  if (!isObject(t) || typeof t.field !== `string` || !t.field || !isObject(t.from)) return null
  const from: Record<string, string[]> = {}
  for (const state of Object.keys(t.from)) if (isStringList(t.from[state])) from[state] = t.from[state] as string[]
  const who: Record<string, string> = {}
  if (isObject(t.who)) for (const key of Object.keys(t.who)) if (typeof t.who[key] === `string`) who[key] = t.who[key] as string
  return { field: t.field, ...(isStringList(t.initial) ? { initial: t.initial } : {}), from, who }
}

/** The states a new record may start in: `initial`, else every `from` state
 *  no edge enters (else every `from` state), in code-unit order. */
export function initialStates(t: RecordTransitions): string[] {
  if (t.initial) return [...t.initial]
  const states = Object.keys(t.from).sort()
  const entered = new Set(Object.values(t.from).flat())
  const sources = states.filter((s) => !entered.has(s))
  return sources.length ? sources : states
}

const stateOf = (data: Record<string, unknown> | null | undefined, field: string): string | null => {
  const v = data?.[field]
  return typeof v === `string` && v !== `` ? v : null
}

/** The level of the edge a → b (`who["a->b"]`, else `who["b"]`, else member). */
function edgeLevel(t: RecordTransitions, a: string, b: string): string {
  const who = t.who ?? {}
  if (own(who, `${a}${ARROW}${b}`)) return who[`${a}${ARROW}${b}`]!
  if (own(who, b)) return who[b]!
  return `member`
}

/** Why the viewer may not take an edge of `level` (null = they may). */
function levelRefusal(level: string, field: string, to: string, viewer: TransitionViewer, before: Record<string, unknown> | null, row: RecordOwnership | null | undefined): string | null {
  if (level === `member`) return null
  if (viewer.role === `owner`) return null
  if (level === `owners`) return `Only team owners can move ${field} to ${to}`
  if (level === `author`) {
    const author = recordAuthorOf(row)
    return author !== null && author === viewer.id ? null : `Only the record's author or a team owner can move ${field} to ${to}`
  }
  if (level.startsWith(`role:`)) {
    const roleField = level.slice(5)
    const holder = stateOf(before, roleField)
    return holder !== null && holder === viewer.id ? null : `Only the record's ${roleField} or a team owner can move ${field} to ${to}`
  }
  return `Nobody can move ${field} to ${to}`
}

/** Why a write of a kind with transitions `t` may not land: `before` = the
 *  stored data (null = a create), `after` = the data as it would be stored,
 *  `row` = the record's ownership (an update's). Null = it may. Order: an
 *  unchanged state passes; a create (or a record without a state yet) starts
 *  in an initial state; a cleared state or an undeclared edge = BAD_REQUEST;
 *  the edge's `who` = FORBIDDEN. */
export function transitionRefusal(t: RecordTransitions | null, before: Record<string, unknown> | null, after: Record<string, unknown>, viewer: TransitionViewer, row?: RecordOwnership | null): TransitionRefusal | null {
  if (!t) return null
  const from = stateOf(before, t.field)
  const to = stateOf(after, t.field)
  if (from === to) return null
  if (from === null) {
    const initial = initialStates(t)
    return to === null || initial.includes(to) ? null : { code: `BAD_REQUEST`, message: `${t.field}: a new record starts in ${initial.join(`, `)}` }
  }
  if (to === null || !(own(t.from, from) && t.from[from]!.includes(to)))
    return { code: `BAD_REQUEST`, message: `${t.field}: ${from} → ${to ?? `(none)`} is not a declared transition` }
  const refusal = levelRefusal(edgeLevel(t, from, to), t.field, to, viewer, before, row)
  return refusal ? { code: `FORBIDDEN`, message: refusal } : null
}

/** The next states a record may move to (in the declared order; null = a
 *  new record: the initial states), only those the viewer may take when one
 *  is given. What a host offers (a template's choices). */
export function transitionsFrom(t: RecordTransitions | null, record: Record<string, unknown> | null, viewer?: TransitionViewer | null, row?: RecordOwnership | null): string[] {
  if (!t) return []
  const from = stateOf(record, t.field)
  if (from === null) return initialStates(t)
  const next = own(t.from, from) ? t.from[from]! : []
  return viewer ? next.filter((to) => levelRefusal(edgeLevel(t, from, to), t.field, to, viewer, record, row) === null) : [...next]
}

/** eApps R6: a kind's `transitions`, each issue under `<at>/transitions`:
 *  the field is a declared enum string field, every state (`from` keys,
 *  their next states, `initial`) one of its values, a `who` key a declared
 *  edge (`a->b`) or a state, its level member, author, owners or
 *  `role:<field>` with the field a `member` ref of the kind. */
export function validateRecordTransitions(kind: unknown, at: string, kindName: string): PackageIssue[] {
  if (!isObject(kind) || kind.transitions === undefined) return []
  const t = kind.transitions
  const tp = `${at}/transitions`
  if (!isObject(t)) return [{ path: tp, message: `transitions is {field, from, initial?, who?}` }]
  const issues: PackageIssue[] = []
  for (const key of Object.keys(t).sort()) if (!KEYS.includes(key)) issues.push({ path: `${tp}/${key}`, message: `unknown key ${key}` })
  const props = isObject(kind.schema) && isObject(kind.schema.properties) ? (kind.schema.properties as Record<string, unknown>) : {}
  const field = typeof t.field === `string` ? t.field : null
  const def = field !== null && own(props, field) ? props[field] : undefined
  let states: string[] | null = null
  if (field === null || !field) issues.push({ path: `${tp}/field`, message: `field is the name of an enum field` })
  else if (!isObject(def)) issues.push({ path: `${tp}/field`, message: `${field} is not a declared field` })
  else if (def.type !== `string` || !isStringList(def.enum)) issues.push({ path: `${tp}/field`, message: `${field} is not an enum field` })
  else states = def.enum as string[]
  const state = (s: unknown, p: string) => {
    if (typeof s !== `string`) issues.push({ path: p, message: `a state is a string` })
    else if (states && !states.includes(s)) issues.push({ path: p, message: `${s} is not a state of ${field}` })
  }
  if (!isObject(t.from)) issues.push({ path: `${tp}/from`, message: `from is {state: [next state…]}` })
  else
    for (const s of Object.keys(t.from).sort()) {
      state(s, `${tp}/from/${s}`)
      const next = t.from[s]
      if (!Array.isArray(next)) issues.push({ path: `${tp}/from/${s}`, message: `the next states are a list` })
      else next.forEach((n, i) => state(n, `${tp}/from/${s}/${i}`))
    }
  if (t.initial !== undefined) {
    if (!Array.isArray(t.initial) || !t.initial.length) issues.push({ path: `${tp}/initial`, message: `initial is a non-empty list of states` })
    else t.initial.forEach((s, i) => state(s, `${tp}/initial/${i}`))
  }
  if (t.who !== undefined) {
    if (!isObject(t.who)) issues.push({ path: `${tp}/who`, message: `who is {"a->b" | state: level}` })
    else {
      const refs = isObject(kind.refs) ? kind.refs : {}
      for (const key of Object.keys(t.who).sort()) {
        const wp = `${tp}/who/${key}`
        const arrow = key.indexOf(ARROW)
        if (arrow >= 0) {
          const a = key.slice(0, arrow)
          const b = key.slice(arrow + ARROW.length)
          if (!(isObject(t.from) && Array.isArray(t.from[a]) && (t.from[a] as unknown[]).includes(b))) issues.push({ path: wp, message: `${a} → ${b} is not a declared transition` })
        } else state(key, wp)
        const level: unknown = (t.who as Record<string, unknown>)[key]
        if (typeof level === `string` && level.startsWith(`role:`)) {
          const roleField = level.slice(5)
          if (!(own(refs, roleField) && refs[roleField] === `member`)) issues.push({ path: wp, message: `${level}: ${roleField} is not a member ref of ${kindName}` })
        } else if (typeof level !== `string` || !TRANSITION_LEVELS.includes(level)) issues.push({ path: wp, message: `a level is member, author, owners or role:<field>` })
      }
    }
  }
  const computed = isObject(kind.computed) ? kind.computed : {}
  if (own(props, RECORD_TRANSITIONS_ROW_KEY) || own(computed, RECORD_TRANSITIONS_ROW_KEY))
    issues.push({ path: tp, message: `a kind with transitions declares no field ${RECORD_TRANSITIONS_ROW_KEY} (its rows carry the next states there)` })
  return issues
}
