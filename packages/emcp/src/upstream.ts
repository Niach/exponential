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

// VAPP-254: an app's per-app server mapping (`mapping` in the fixture: its
// `$comment` is the spec).

export const UPSTREAM_MAPPING = fixture.mapping

export interface UpstreamServerRef {
  id: string
  name: string
}

export type UpstreamResolution<T> = { server: T; source: `mapped` | `name` }

/** The team server filling a declared upstream: a binding on the team, else
 *  the server of exactly that name, else null. */
export function resolveUpstreamServer<T extends UpstreamServerRef>(upstream: string, bound: string | null, servers: readonly T[]): UpstreamResolution<T> | null {
  const mapped = bound === null ? undefined : servers.find((s) => s.id === bound)
  if (mapped) return { server: mapped, source: `mapped` }
  const named = servers.find((s) => s.name === upstream)
  return named ? { server: named, source: `name` } : null
}

export type UpstreamMappingWrite = { op: `set`; serverId: string } | { op: `clear` } | { error: string }

/** What an owner's pick writes: the exact-name default keeps no row. */
export function upstreamMappingWrite(upstream: string, serverId: string | null, servers: readonly UpstreamServerRef[]): UpstreamMappingWrite {
  if (serverId === null) return { op: `clear` }
  const server = servers.find((s) => s.id === serverId)
  if (!server) return { error: UPSTREAM_ERRORS.missing }
  return server.name === upstream ? { op: `clear` } : { op: `set`, serverId }
}

/** The audit row's `tool` for a mapping change. */
export function upstreamMappingTool(upstream: string, server: string | null): string {
  return `upstream:${upstream}=${server ?? `auto`}`.slice(0, 128)
}

// VAPP-230 (eApps M1): `upstream:<server>/<tool>?<args>` read bindings
// (`binding` in the fixture: its `$comment` is the spec).

export const UPSTREAM_BINDING = {
  scheme: fixture.binding.scheme,
  cacheSeconds: fixture.binding.cacheSeconds,
  banner: fixture.binding.banner,
  connect: fixture.binding.connect,
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
export type UpstreamReadAnswer = { ok: true; value: unknown } | { ok: false; error: string; server: string; offline?: string }

/** The host's banners: one per server whose read answered
 *  upstreamDisconnected, sorted, each once. */
export function upstreamBanners(answers: readonly UpstreamReadAnswer[]): string[] {
  const servers = new Set(answers.flatMap((a) => (!a.ok && a.error === UPSTREAM_ERRORS.disconnected ? [a.server] : [])))
  return [...servers].sort().map((server) => UPSTREAM_BINDING.banner.replace(`{server}`, server))
}

// VAPP-239 (eApps M2): device-routed upstreams (`device` in the fixture: its
// `$comment` is the spec).

const device = fixture.device

export const UPSTREAM_DEVICE = {
  ttlSeconds: device.ttlSeconds,
  answerTimeoutMs: device.answerTimeoutMs,
  maxAnswerBytes: device.maxAnswerBytes,
  errors: device.errors,
  copy: device.copy,
} as const

const fill = (template: string, vars: Record<string, string>): string => template.replace(/\{(\w+)\}/g, (m, k: string) => vars[k] ?? m)

/** A server row as the router reads it: its transport and the device it
 *  names (with the device's owner and label), if any. */
export interface RoutedServer {
  name: string
  transport: string
  device: { id: string; ownerId: string; label: string } | null
}

export type UpstreamRoute = { route: `server` } | { route: `device`; deviceId: string } | { error: string; message: string }

/** Where one call to `server` by `callerId` (the member, or an automation's
 *  sponsor) runs: the server, the owner's device, or a refusal. */
export function upstreamRoute(server: RoutedServer, callerId: string): UpstreamRoute {
  if (!server.device) {
    if (server.transport === `http`) return { route: `server` }
    return { error: device.errors.noDevice, message: fill(device.copy.noDevice, { server: server.name }) }
  }
  if (server.device.ownerId !== callerId) {
    return { error: device.errors.notOwner, message: fill(device.copy.notOwner, { server: server.name, device: server.device.label }) }
  }
  return { route: `device`, deviceId: server.device.id }
}

/** What the relay answered `POST /devices/:userId/:deviceId/upstream` with. */
export type DeviceRelayOutcome = { delivered: true } | { delivered: false; reason: string }

/** The device's (opened) answer: the tool's result, or why it did not run. */
export type DeviceAnswer = { ok: true; result: Record<string, unknown> } | { ok: false; reason: `vetoed` | `refused` | `failed`; error: string }

export type DeviceCallOutcome = { result: Record<string, unknown> } | { error: string; message: string; offline?: string }

/** One device call's outcome for the broker. `answer` = null when the relay
 *  delivered nothing that opens. */
export function deviceCallOutcome(server: string, deviceLabel: string, relay: DeviceRelayOutcome, answer: DeviceAnswer | null): DeviceCallOutcome {
  const vars = { server, device: deviceLabel }
  if (!relay.delivered && relay.reason === `device_offline`) {
    return { error: device.errors.offline, message: fill(device.copy.offline, vars), offline: deviceLabel }
  }
  if (!relay.delivered || !answer) return { error: device.errors.noAnswer, message: fill(device.copy.noAnswer, vars) }
  if (answer.ok) return { result: answer.result }
  if (answer.reason === `vetoed`) {
    const destructive = answer.error === UPSTREAM_BINDING.errors.destructive
    const known = destructive || answer.error === `accessTooLow` || answer.error === UPSTREAM_BINDING.errors.notReadOnly
    return {
      error: known ? answer.error : UPSTREAM_BINDING.errors.notReadOnly,
      message: `${server}'s tool is ${destructive ? `destructive` : `not read-only`}: it does not run here`,
    }
  }
  if (answer.reason === `refused`) return { error: UPSTREAM_ERRORS.disconnected, message: `${server} refused your connection; reconnect` }
  return { error: UPSTREAM_ERRORS.failed, message: `${server}: ${answer.error}` }
}

/** Whether a device may claim a call: the member's own, on that device,
 *  unexpired (`expiresAt` ms) and not claimed yet. */
export function deviceCallClaimable(
  call: { userId: string; deviceId: string; expiresAt: number; claimed: boolean },
  claim: { userId: string; deviceId: string },
  now: number
): boolean {
  return !call.claimed && call.expiresAt > now && call.userId === claim.userId && call.deviceId === claim.deviceId
}

/** One host banner: its text and whether it carries the Connect action. */
export interface UpstreamHostBanner {
  server: string
  text: string
  connect: boolean
}

/** The host's banners ×4: `Connect <server>` for an upstreamDisconnected
 *  read, the offline copy for a device that is offline; one per server,
 *  sorted by server. */
export function upstreamHostBanners(answers: readonly UpstreamReadAnswer[]): UpstreamHostBanner[] {
  const byServer = new Map<string, UpstreamHostBanner>()
  for (const a of answers) {
    if (a.ok || byServer.has(a.server)) continue
    if (a.error === UPSTREAM_ERRORS.disconnected) byServer.set(a.server, { server: a.server, text: UPSTREAM_BINDING.banner.replace(`{server}`, a.server), connect: true })
    else if (a.error === device.errors.offline && a.offline) byServer.set(a.server, { server: a.server, text: fill(device.copy.offline, { server: a.server, device: a.offline }), connect: false })
  }
  return [...byServer.values()].sort((x, y) => (x.server < y.server ? -1 : x.server > y.server ? 1 : 0))
}

const LAN_SUFFIXES = [`.local`, `.lan`, `.home.arpa`, `.internal`]

function privateIpv4(host: string): boolean {
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(host)
  if (!m) return false
  const [a, b] = [Number(m[1]), Number(m[2])]
  if (m.slice(1).some((p) => Number(p) > 255)) return false
  return a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168)
}

/** A server row's URL rule: https anywhere; http on loopback or a LAN host
 *  (a device-routed server's reach). */
export function mcpUrlAllowed(value: string): boolean {
  let url: URL
  try {
    url = new URL(value.trim())
  } catch {
    return false
  }
  if (url.protocol === `https:`) return true
  if (url.protocol !== `http:`) return false
  const host = url.hostname.toLowerCase()
  return host === `localhost` || host === `127.0.0.1` || privateIpv4(host) || LAN_SUFFIXES.some((suffix) => host.endsWith(suffix))
}
