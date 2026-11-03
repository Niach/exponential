// EXP-1490: one member's connection to one upstream (a team MCP server an
// app binds to). Pure and byte-locked by `fixtures/upstream.json` (its
// `$comment` is the spec).

import fixture from "../fixtures/upstream.json" with { type: "json" }
import { canonicalJson, upstreamDestructive } from "./pure"

export const UPSTREAM_STATES = fixture.states as readonly UpstreamState[]
export const UPSTREAM_LABELS = fixture.labels as Record<UpstreamState, string>
export const UPSTREAM_ERRORS = fixture.errors

export type UpstreamState = `disconnected` | `connecting` | `connected` | `expired` | `reconnect` | `failed`
export type UpstreamEvent =
  | `connect`
  | `callbackOk`
  | `callbackError`
  | `flowExpired`
  | `secretSet`
  | `expire`
  | `refreshOk`
  | `refreshFailed`
  | `unauthorized`
  | `disconnect`
  | `serverChanged`

const TRANSITIONS = fixture.transitions as Record<string, Record<string, UpstreamState>>

/** The next state; an event a state does not list changes nothing. */
export function upstreamStep(state: UpstreamState, event: UpstreamEvent): UpstreamState {
  const any = TRANSITIONS[`*`]?.[event]
  if (any) return any
  return TRANSITIONS[state]?.[event] ?? state
}

/** May the broker call through it (an expired token is refreshed first)? */
export function upstreamCallable(state: UpstreamState): boolean {
  return (fixture.callable as readonly string[]).includes(state)
}

/** The upstreams the consent page offers to connect: every server the
 *  chosen apps declare whose member state is not callable (no state =
 *  `disconnected`), sorted, each once. A server that no longer exists is
 *  not offered (its bindings answer `upstreamMissing`). */
export function missingUpstreams(
  declared: readonly (readonly string[])[],
  states: Readonly<Record<string, UpstreamState>>,
  servers: readonly string[]
): string[] {
  const names = new Set(declared.flat().filter((name) => servers.includes(name)))
  return [...names].filter((name) => !upstreamCallable(states[name] ?? `disconnected`)).sort()
}

// VAPP-230 (eApps M1): `upstream:<server>/<tool>?<args>` read bindings
// (`binding` in the fixture: its `$comment` is the spec).

export const UPSTREAM_BINDING = {
  scheme: fixture.binding.scheme,
  cacheSeconds: fixture.binding.cacheSeconds,
  banner: fixture.binding.banner,
  errors: fixture.binding.errors,
} as const

export interface UpstreamBinding {
  server: string
  tool: string
  args: Record<string, unknown>
}

const SOURCE = /^([a-zA-Z][a-zA-Z0-9+.-]*):([^?]+)(?:\?(.*))?$/
const JSON_NUMBER = /^-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?$/

function decode(part: string): string {
  try {
    return decodeURIComponent(part.replace(/\+/g, ` `))
  } catch {
    return part
  }
}

/** A param's text as an argument: JSON literals, numbers, objects and
 *  arrays read as JSON; anything else stays the string. */
export function upstreamArg(text: string): unknown {
  if (text === `true`) return true
  if (text === `false`) return false
  if (text === `null`) return null
  if (JSON_NUMBER.test(text)) return Number(text)
  if (text.startsWith(`{`) || text.startsWith(`[`)) {
    try {
      return JSON.parse(text)
    } catch {
      return text
    }
  }
  return text
}

/** `upstream:crm/search?q=acme` → `{server: "crm", tool: "search", args:
 *  {q: "acme"}}` (eUI's source grammar; the name splits at its LAST `/`),
 *  null when it is not an upstream source. */
export function parseUpstreamSource(source: string): UpstreamBinding | null {
  const m = SOURCE.exec(source.trim())
  if (!m || m[1]!.toLowerCase() !== UPSTREAM_BINDING.scheme) return null
  const name = decode(m[2]!)
  const slash = name.lastIndexOf(`/`)
  if (slash <= 0 || slash === name.length - 1) return null
  const params: Record<string, string> = {}
  for (const pair of (m[3] ?? ``).split(`&`)) {
    if (!pair) continue
    const eq = pair.indexOf(`=`)
    const key = decode(eq < 0 ? pair : pair.slice(0, eq))
    if (!key) continue
    params[key] = eq < 0 ? `` : decode(pair.slice(eq + 1))
  }
  const args: Record<string, unknown> = {}
  for (const key of Object.keys(params).sort()) args[key] = upstreamArg(params[key]!)
  return { server: name.slice(0, slash), tool: name.slice(slash + 1), args }
}

/** Exponential's install check: every `upstream:` source of a package's
 *  template bindings parses and names one of its `upstreams`. */
export function upstreamDeclareIssues(sources: readonly string[], upstreams: readonly string[]): { index: number; code: string }[] {
  const issues: { index: number; code: string }[] = []
  sources.forEach((source, index) => {
    if (!new RegExp(`^${UPSTREAM_BINDING.scheme}:`, `i`).test(source.trim())) return
    const parsed = parseUpstreamSource(source)
    if (!parsed) issues.push({ index, code: UPSTREAM_BINDING.errors.invalid })
    else if (!upstreams.includes(parsed.server)) issues.push({ index, code: UPSTREAM_BINDING.errors.notDeclared })
  })
  return issues
}

/** The broker's call-time check against the tool's live annotations and the
 *  package's `sources` (an exact `upstream:<server>/<tool>` vouches for a
 *  tool that sets no readOnlyHint): null = it reads, else the error. */
export function upstreamReadError(
  binding: Pick<UpstreamBinding, `server` | `tool`>,
  annotations: { readOnlyHint?: unknown; destructiveHint?: unknown } | null | undefined,
  sources: readonly string[]
): string | null {
  if (upstreamDestructive(binding.tool, annotations as { destructiveHint?: boolean } | null | undefined)) return UPSTREAM_BINDING.errors.destructive
  if (annotations?.readOnlyHint === true) return null
  if (annotations?.readOnlyHint === false) return UPSTREAM_BINDING.errors.notReadOnly
  return sources.includes(`${UPSTREAM_BINDING.scheme}:${binding.server}/${binding.tool}`) ? null : UPSTREAM_BINDING.errors.notReadOnly
}

/** The cache key of one read: member, server id, tool and canonical args. */
export function upstreamCacheKey(member: string, serverId: string, tool: string, args: Record<string, unknown>): string {
  return [member, serverId, tool, canonicalJson(args)].join(`\n`)
}

/** An MCP tools/call result as the binding's value. */
export function upstreamBindingValue(result: Record<string, unknown>): { value: unknown } | { error: string } {
  if (result.isError === true) return { error: UPSTREAM_ERRORS.failed }
  const structured = result.structuredContent
  if (structured && typeof structured === `object` && !Array.isArray(structured)) return { value: structured }
  const text = Array.isArray(result.content)
    ? (result.content.find((c) => c && typeof c === `object` && (c as { type?: unknown }).type === `text` && typeof (c as { text?: unknown }).text === `string`) as { text: string } | undefined)
    : undefined
  if (!text) return { value: null }
  try {
    return { value: JSON.parse(text.text) }
  } catch {
    return { value: text.text }
  }
}

/** One host answer of `apps.upstreamRead`. */
export type UpstreamReadAnswer = { ok: true; value: unknown } | { ok: false; error: string; server: string }

/** The host's banners: one per server whose read answered
 *  upstreamDisconnected, sorted, each once. */
export function upstreamBanners(answers: readonly UpstreamReadAnswer[]): string[] {
  const servers = new Set(answers.flatMap((a) => (!a.ok && a.error === UPSTREAM_ERRORS.disconnected ? [a.server] : [])))
  return [...servers].sort().map((server) => UPSTREAM_BINDING.banner.replace(`{server}`, server))
}
