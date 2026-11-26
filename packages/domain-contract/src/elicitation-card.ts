// #NEW-146 (fixtures/elicitation-card.json, its `$comment` is the spec):
// a THIRD-PARTY MCP server's elicitation as THE question card ×4. The
// DEVICE builds the card (`elicitationCard`) from the ACP elicitation and
// posts it like every engine ask (sealed, Needs you, push); the renderers
// draw the card's genUI surface, its server line and its buttons, and the
// device maps the answered card back (`elicitationAction`,
// `elicitationContent`). Mirrors: the engine (eagent elicitation_card.rs:
// cardCases, contentCases, actionCases, urlCases), desktop domain
// elicitation_card.rs, iOS ExpCore ElicitationCard.swift, Android
// domain/ElicitationCard.kt (missingCases, actionCases, urlCases).

import fixture from "../fixtures/elicitation-card.json" with { type: "json" }

export const ELICITATION_CARD_COPY = fixture.copy
export const ELICITATION_CARD_ICONS = fixture.icons
export const ELICITATION_SERVER_META_KEY = fixture.serverMetaKey
export const ELICITATION_ORDER_META_KEY = fixture.orderMetaKey
/** The surface's data model root: every field binds `/<formKey>/<name>`. */
export const ELICITATION_FORM_KEY = fixture.formKey

/** One MCP `requestedSchema` property (the primitive subset MCP allows). */
export interface ElicitationProperty {
  type: `string` | `number` | `integer` | `boolean` | `array`
  title?: string | null
  description?: string | null
  format?: `email` | `uri` | `date` | `date-time` | null
  enum?: string[] | null
  enumNames?: string[] | null
  oneOf?: { const: string; title: string }[] | null
  items?: { enum?: string[] | null; anyOf?: { const: string; title: string }[] | null } | null
  minimum?: number | null
  maximum?: number | null
  default?: unknown
}

export interface ElicitationSchema {
  properties: Record<string, ElicitationProperty>
  required?: string[] | null
}

export type ElicitationRequest =
  | { server: string; message: string; mode: `form`; schema: ElicitationSchema; order?: string[] | null }
  | { server: string; message: string; mode: `url`; url: string; elicitationId: string }

export type ElicitationUrlWarning = `punycode` | `ip` | `insecure`

export interface ElicitationUrl {
  /** The URL exactly as the server sent it (trimmed). */
  url: string
  host: string
  /** The registrable domain (eTLD+1; an IP or a one-label host = itself). */
  domain: string
  warnings: ElicitationUrlWarning[]
}

/** The card's sidecar: what the renderers need beyond the surface. */
export interface ElicitationSidecar {
  server: string
  mode: `form` | `url`
  /** The required field names (form): Submit waits for every one. */
  required: string[]
  /** The full URL (url mode): THE Open link target. */
  url: string | null
}

export interface ElicitationCard {
  kind: `question`
  question: string
  options: string[]
  custom: false
  multi: false
  surface: { root: Record<string, unknown>; data: Record<string, unknown> }
  elicitation: ElicitationSidecar
}

type Node = Record<string, unknown>

const bind = (name: string) => ({ path: `/${ELICITATION_FORM_KEY}/${name}` })

/** Untrimmed text, else null. */
const text = (value: unknown): string | null =>
  typeof value === `string` && value.trim() !== `` ? value.trim() : null

/** The property's options as genUI Options ([] = not a choice). */
function choiceOptions(property: ElicitationProperty): { label: string; value: string }[] {
  if (property.type === `array`) {
    const items = property.items ?? {}
    if (items.anyOf?.length) return items.anyOf.map((o) => ({ label: o.title, value: o.const }))
    return (items.enum ?? []).map((v) => ({ label: v, value: v }))
  }
  if (property.oneOf?.length) return property.oneOf.map((o) => ({ label: o.title, value: o.const }))
  if (property.enum?.length) {
    return property.enum.map((v, i) => ({ label: property.enumNames?.[i] ?? v, value: v }))
  }
  return []
}

/** A property's starting value: its default when it has the right type,
 *  else empty ("" / null / false / []). */
function startValue(property: ElicitationProperty): unknown {
  const given = property.default
  switch (property.type) {
    case `boolean`:
      return typeof given === `boolean` ? given : false
    case `number`:
    case `integer`:
      return typeof given === `number` && Number.isFinite(given) ? given : null
    case `array`:
      return Array.isArray(given) ? given.filter((v) => typeof v === `string`) : []
    default:
      return typeof given === `string` ? given : ``
  }
}

/** ONE genUI field per property (the spec's FIELD MAP). */
function field(name: string, property: ElicitationProperty): Node {
  const label = text(property.title) ?? name
  const description = text(property.description)
  const id = `f_${name}`
  const options = choiceOptions(property)
  if (property.type === `boolean`) {
    return { id, component: `Switch`, props: { label, name, checked: bind(name), ...(description ? { description } : {}) } }
  }
  if (property.type === `array` || options.length > 0) {
    return {
      id,
      component: `Select`,
      props: {
        label,
        name,
        options,
        value: bind(name),
        ...(property.type === `array` ? { multiple: true } : {}),
        ...(description ? { description } : {}),
      },
    }
  }
  if (property.type === `number` || property.type === `integer`) {
    return {
      id,
      component: `NumberField`,
      props: {
        label,
        name,
        value: bind(name),
        ...(typeof property.minimum === `number` ? { min: property.minimum } : {}),
        ...(typeof property.maximum === `number` ? { max: property.maximum } : {}),
        ...(property.type === `integer` ? { step: 1, precision: 0 } : {}),
        ...(description ? { placeholder: description } : {}),
      },
    }
  }
  if (property.format === `date`) {
    return { id, component: `DatePicker`, props: { label, name, value: bind(name), ...(description ? { placeholder: description } : {}) } }
  }
  const type = property.format === `email` ? `email` : property.format === `uri` ? `url` : `text`
  return {
    id,
    component: `Input`,
    props: { label, name, type, value: bind(name), ...(description ? { placeholder: description } : {}) },
  }
}

/** The property names in the server's order (`order`, the names it lists
 *  first), then any the order missed by name. */
export function elicitationFieldOrder(schema: ElicitationSchema, order?: readonly string[] | null): string[] {
  const names = Object.keys(schema.properties)
  const first = (order ?? []).filter((name, i, all) => names.includes(name) && all.indexOf(name) === i)
  return [...first, ...names.filter((name) => !first.includes(name)).sort()]
}

const IPV4 = /^\d{1,3}(\.\d{1,3}){3}$/

/** The URL review (url mode): `null` = not an absolute http(s) URL (the
 *  device declines it; nothing else is ever opened). */
export function elicitationUrl(raw: string): ElicitationUrl | null {
  const url = raw.trim()
  let parsed: URL
  try {
    parsed = new URL(url)
  } catch {
    return null
  }
  if (parsed.protocol !== `https:` && parsed.protocol !== `http:`) return null
  const host = parsed.hostname.toLowerCase()
  if (host === ``) return null
  const ip = IPV4.test(host) || host.startsWith(`[`)
  const labels = host.split(`.`)
  const warnings: ElicitationUrlWarning[] = []
  if (!ip && labels.some((l) => l.startsWith(`xn--`))) warnings.push(`punycode`)
  if (ip) warnings.push(`ip`)
  if (parsed.protocol !== `https:`) warnings.push(`insecure`)
  let domain = host
  if (!ip && labels.length > 2) {
    const tail2 = labels.slice(-2).join(`.`)
    domain = fixture.multiPartSuffixes.includes(tail2) ? labels.slice(-3).join(`.`) : tail2
  }
  return { url, host, domain, warnings }
}

/** THE card a device posts for an elicitation; `null` = it cannot be shown
 *  honestly (a url mode whose URL is not http(s)): the device declines. */
export function elicitationCard(request: ElicitationRequest): ElicitationCard | null {
  const server = text(request.server) ?? fixture.copy.unknownServer
  const question = text(request.message) ?? ``
  if (request.mode === `url`) {
    const review = elicitationUrl(request.url)
    if (!review) return null
    const children: Node[] = [
      { id: `domain`, component: `Heading`, props: { text: review.domain, level: `h4` } },
      { id: `url`, component: `Text`, props: { text: review.url, variant: `code` } },
      ...review.warnings.map((w) => ({
        id: `warn_${w}`,
        component: `Alert`,
        props: { title: fixture.copy.warnings[w], type: `warning` },
      })),
    ]
    return {
      kind: `question`,
      question,
      options: [fixture.copy.done, fixture.copy.decline],
      custom: false,
      multi: false,
      surface: { root: { id: `root`, component: `Stack`, props: { direction: `vertical`, gap: `sm` }, children }, data: {} },
      elicitation: { server, mode: `url`, required: [], url: review.url },
    }
  }
  const names = elicitationFieldOrder(request.schema, request.order)
  const values: Record<string, unknown> = {}
  for (const name of names) values[name] = startValue(request.schema.properties[name]!)
  return {
    kind: `question`,
    question,
    options: [fixture.copy.submit, fixture.copy.decline, fixture.copy.cancel],
    custom: false,
    multi: false,
    surface: {
      root: {
        id: `root`,
        component: `Stack`,
        props: { direction: `vertical`, gap: `md` },
        children: names.map((name) => field(name, request.schema.properties[name]!)),
      },
      data: { [ELICITATION_FORM_KEY]: values },
    },
    elicitation: {
      server,
      mode: `form`,
      required: (request.schema.required ?? []).filter((name) => names.includes(name)),
      url: null,
    },
  }
}

/** A value counts as given: a non-blank string, a finite number, any
 *  boolean, a non-empty list. */
function given(value: unknown): boolean {
  if (typeof value === `string`) return value.trim() !== ``
  if (typeof value === `number`) return Number.isFinite(value)
  if (typeof value === `boolean`) return true
  if (Array.isArray(value)) return value.length > 0
  return false
}

const formValues = (surfaceData: Record<string, unknown> | null | undefined): Record<string, unknown> => {
  const form = surfaceData?.[ELICITATION_FORM_KEY]
  return form && typeof form === `object` && !Array.isArray(form) ? (form as Record<string, unknown>) : {}
}

/** The required fields still empty (renderers: Submit is disabled while
 *  any is, `copy.missing` under the buttons). */
export function elicitationMissing(
  sidecar: Pick<ElicitationSidecar, `required`>,
  surfaceData: Record<string, unknown> | null | undefined
): string[] {
  const form = formValues(surfaceData)
  return sidecar.required.filter((name) => !given(form[name]))
}

export type ElicitationAction = `accept` | `decline` | `cancel`

/** The action an answered (or dismissed) elicitation card carries back:
 *  Submit / I'm done = accept, Decline = decline, anything else (Cancel,
 *  the ×, a label we never offered) = cancel. Never an accept by default. */
export function elicitationAction(card: {
  answers?: readonly string[] | null
  answer?: string | null
  dismissed?: boolean | null
}): ElicitationAction {
  if (card.dismissed === true) return `cancel`
  const pick = (card.answers?.[0] ?? card.answer ?? ``).trim()
  if (pick === fixture.copy.submit || pick === fixture.copy.done) return `accept`
  if (pick === fixture.copy.decline) return `decline`
  return `cancel`
}

/** The accepted content: each property's value from the surface data,
 *  coerced to its type; empty optional values are left out. `null` = a
 *  required value is missing or a value has the wrong type (never sent). */
export function elicitationContent(
  schema: ElicitationSchema,
  surfaceData: Record<string, unknown> | null | undefined
): Record<string, unknown> | null {
  const form = formValues(surfaceData)
  const required = new Set(schema.required ?? [])
  const out: Record<string, unknown> = {}
  for (const name of elicitationFieldOrder(schema)) {
    const property = schema.properties[name]!
    const value = form[name]
    if (!given(value)) {
      if (required.has(name)) return null
      continue
    }
    switch (property.type) {
      case `boolean`:
        if (typeof value !== `boolean`) return null
        out[name] = value
        break
      case `integer`:
      case `number`: {
        const number = typeof value === `number` ? value : Number(String(value).trim())
        if (!Number.isFinite(number)) return null
        if (property.type === `integer` && !Number.isInteger(number)) return null
        out[name] = number
        break
      }
      case `array`: {
        if (!Array.isArray(value) || value.some((v) => typeof v !== `string`)) return null
        const allowed = choiceOptions(property).map((o) => o.value)
        if (allowed.length > 0 && value.some((v) => !allowed.includes(v as string))) return null
        out[name] = value
        break
      }
      default: {
        if (typeof value !== `string`) return null
        const allowed = choiceOptions(property).map((o) => o.value)
        if (allowed.length > 0 && !allowed.includes(value)) return null
        out[name] = value.trim()
      }
    }
  }
  return out
}
