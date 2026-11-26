// #NEW-145 / #NEW-143 D6: elicitation on Exponential's MCP server: the
// confirm before every delete and stop, the team and device picks, the URL
// flows and the no-secret rule. Pure (no I/O; the requestState half lives in
// request-state.ts) and locked by `fixtures/elicitation.json` (its
// `$comment` is the spec).

import fixture from "../fixtures/elicitation.json" with { type: "json" }
import { destructiveCall, singleIdError } from "./deletes"

export interface ElicitationModes {
  form: boolean
  url: boolean
}

/** The modes a request's client capabilities declare (`_meta[capabilityKey]`). */
export function elicitationModes(capabilities: unknown): ElicitationModes {
  const e = (capabilities as { elicitation?: unknown } | null | undefined)?.elicitation
  if (!e || typeof e !== `object` || Array.isArray(e)) return { form: false, url: false }
  const has = (key: string) => key in (e as Record<string, unknown>)
  const any = has(`form`) || has(`url`)
  return { form: any ? has(`form`) : true, url: has(`url`) }
}

/** A JSON Schema object of one flat form. */
export interface FormSchema {
  type: `object`
  properties: Record<string, Record<string, unknown>>
  required?: string[]
}

export interface ElicitRequest {
  method: `elicitation/create`
  params: { mode: `form`; message: string; requestedSchema: FormSchema } | { mode: `url`; message: string; url: string }
}

/** An MRTR answer the client sent for one key. */
export interface ElicitResult {
  action: `accept` | `decline` | `cancel`
  content?: Record<string, unknown>
}

const fill = (template: string, values: Record<string, string>) =>
  template.replace(/\{(\w+)\}/g, (whole, key: string) => (key in values ? values[key]! : whole))

export const CONFIRM_KEY: string = fixture.confirm.key
export const CONFIRM_SCHEMA = fixture.confirm.schema as FormSchema
export const ELICIT_RESULTS = fixture.results

const SESSIONS_SEND = `exponential_sessions_send`

/** The confirm a call needs, or null: a destructive op with one valid id, or a session stop. */
export function confirmFor(tool: string, args: unknown): { message: string } | null {
  const nouns = fixture.confirm.nouns as Record<string, string>
  const messages = fixture.confirm.messages as Record<string, string>
  const cut = (id: string) => id.slice(0, fixture.confirm.idMax)
  if (tool === SESSIONS_SEND) {
    const a = (args ?? {}) as Record<string, unknown>
    if (a.stop !== true || typeof a.sessionId !== `string` || singleIdError(a.sessionId)) return null
    return { message: fill(messages.stop!, { noun: nouns.sessions!, id: cut(a.sessionId.trim()) }) }
  }
  const call = destructiveCall(tool, args)
  if (!call || singleIdError(call.value)) return null
  const noun = nouns[`${call.noun}.${call.op}`] ?? nouns[call.noun] ?? call.noun
  const template = messages[call.op] ?? messages.delete!
  return { message: fill(template, { noun, id: cut(String(call.value).trim()) }) }
}

export type GateVerdict = { kind: `confirm`; message: string; request: ElicitRequest } | { kind: `none` } | { kind: `legacy` }

/** Whether a call asks before it runs: `confirm` (form), `legacy` (it would, but the client has no form mode) or `none`. */
export function confirmGate(tool: string, args: unknown, modes: ElicitationModes): GateVerdict {
  const confirm = confirmFor(tool, args)
  if (!confirm) return { kind: `none` }
  if (!modes.form) return { kind: `legacy` }
  return {
    kind: `confirm`,
    message: confirm.message,
    request: { method: `elicitation/create`, params: { mode: `form`, message: confirm.message, requestedSchema: CONFIRM_SCHEMA } },
  }
}

const isResult = (r: unknown): r is ElicitResult =>
  !!r && typeof r === `object` && !Array.isArray(r) && [`accept`, `decline`, `cancel`].includes((r as { action?: unknown }).action as string)

export type ConfirmOutcome = `run` | `declined` | `cancelled` | `reask` | `invalid`

/** What a confirm's answer does. */
export function confirmOutcome(response: unknown): ConfirmOutcome {
  if (response === undefined || response === null) return `reask`
  if (!isResult(response)) return `invalid`
  if (response.action === `cancel`) return `cancelled`
  if (response.action === `decline`) return `declined`
  return response.content?.[CONFIRM_KEY] === true ? `run` : `declined`
}

export type PickKind = `team` | `device`

/** Does this call need a pick (its arguments name no team / device)? */
export function pickNeeded(pick: PickKind, tool: string, args: unknown): boolean {
  const a = (args ?? {}) as Record<string, unknown>
  const absent = (keys: readonly string[]) => keys.every((key) => a[key] === undefined || a[key] === null || a[key] === ``)
  if (pick === `device`) return tool === fixture.devicePick.tool && absent(fixture.devicePick.absent)
  return fixture.teamPick.tools.some((rule) => rule.tool === tool && (rule.op === null || a.op === rule.op) && absent(rule.absent))
}

function compareCodePoints(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0
}

export interface PickChoice {
  id: string
  name: string
}

export type PickVerdict = { kind: `ask`; request: ElicitRequest } | { kind: `use`; id: string } | { kind: `legacy` }

/** A team or device pick over the caller's choices. */
export function pickRequest(pick: PickKind, choices: readonly PickChoice[], modes: ElicitationModes): PickVerdict {
  if (choices.length === 1) return { kind: `use`, id: choices[0]!.id }
  if (choices.length === 0 || !modes.form) return { kind: `legacy` }
  const spec = pick === `team` ? fixture.teamPick : fixture.devicePick
  const oneOf = [...choices]
    .sort((x, y) => compareCodePoints(x.name, y.name) || compareCodePoints(x.id, y.id))
    .map((c) => ({ const: c.id, title: c.name }))
  return {
    kind: `ask`,
    request: {
      method: `elicitation/create`,
      params: {
        mode: `form`,
        message: spec.message,
        requestedSchema: { type: `object`, properties: { [spec.property]: { type: `string`, title: spec.title, oneOf } }, required: [spec.property] },
      },
    },
  }
}

export type PickOutcome = { kind: `use`; id: string } | { kind: `noPick` } | { kind: `reask` } | { kind: `invalid` }

/** What a pick's answer does: only an offered const is used. */
export function pickOutcome(property: string, offered: readonly string[], response: unknown): PickOutcome {
  if (response === undefined || response === null) return { kind: `reask` }
  if (!isResult(response)) return { kind: `invalid` }
  if (response.action !== `accept`) return { kind: `noPick` }
  const value = response.content?.[property]
  return typeof value === `string` && offered.includes(value) ? { kind: `use`, id: value } : { kind: `invalid` }
}

// ── URL mode ──────────────────────────────────────────────────────────

export type UrlPurpose = keyof typeof fixture.url.purposes

const ID_RE = new RegExp(fixture.url.idPattern)

/** `${APP_URL}/mcp/elicit/<id>`. */
export function elicitUrl(appUrl: string, id: string): string {
  return `${appUrl.replace(/\/+$/, ``)}${fixture.url.path}${id}`
}

/** Is `url` an elicitation page of this deployment (same origin, the one path, an opaque id, nothing else)? */
export function urlElicitationAllowed(url: string, appUrl: string): boolean {
  let u: URL
  let app: URL
  try {
    u = new URL(url)
    app = new URL(appUrl)
  } catch {
    return false
  }
  if (u.origin !== app.origin || u.username || u.password || u.search || u.hash || url.includes(`?`) || url.includes(`#`)) return false
  if (!u.pathname.startsWith(fixture.url.path)) return false
  return ID_RE.test(u.pathname.slice(fixture.url.path.length))
}

export type UrlVerdict = { kind: `ask`; request: ElicitRequest } | { kind: `legacy` } | { kind: `refused` }

/** The URL-mode request of one purpose, or `legacy` (no url mode: today's link as text). */
export function urlRequest(purpose: string, subject: string | null, appUrl: string, id: string, modes: ElicitationModes): UrlVerdict {
  const template = (fixture.url.purposes as Record<string, string>)[purpose]
  if (!template) return { kind: `refused` }
  if (!modes.url) return { kind: `legacy` }
  const url = elicitUrl(appUrl, id)
  if (!urlElicitationAllowed(url, appUrl)) return { kind: `refused` }
  return { kind: `ask`, request: { method: `elicitation/create`, params: { mode: `url`, message: fill(template, { name: subject ?? `` }), url } } }
}

// ── No secrets in form mode ───────────────────────────────────────────

const SECRET_RE = new RegExp(fixture.secret.pattern, `i`)

/** The properties of a form schema that look like a secret ([] = fine). */
export function secretFields(schema: { properties?: Record<string, Record<string, unknown>> }): string[] {
  return Object.entries(schema.properties ?? {})
    .filter(([name, prop]) => {
      if (prop.format === `password`) return true
      const words = [name.replace(/([a-z])([A-Z])/g, `$1 $2`), prop.title, prop.description].filter((v): v is string => typeof v === `string`)
      return words.some((text) => SECRET_RE.test(text))
    })
    .map(([name]) => name)
}
