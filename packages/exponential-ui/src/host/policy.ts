// VAPP-91: the policy rules every host applies before a client function, a
// URL open or a media load (`catalog/host.json` functions / urls / media).
// Pure; `fixtures/host-policy.json` locks them on every platform.

import { DEFAULT_URL_SCHEMES } from "./contract"
import { coreCatalog } from "../catalog"

/** The catalog's 14 built-in client functions. */
export const BUILTIN_FUNCTIONS: readonly string[] = (coreCatalog as unknown as { functions: { names: string[] } }).functions.names

export type FunctionDecision = `allow` | `ask` | `deny` | `not_found`

export interface FunctionPolicy {
  allow?: readonly string[]
  ask?: readonly string[]
  deny?: readonly string[]
  /** For a registered name no list matches; default `allow`. */
  default?: `allow` | `ask` | `deny`
}

/** `harness.*` matches `harness.toast`; anything else is exact. */
export function matchesPattern(pattern: string, name: string): boolean {
  return pattern.endsWith(`*`) ? name.startsWith(pattern.slice(0, -1)) : pattern === name
}

/** The gate: built-ins always run; an unregistered name is `not_found`;
 *  then deny wins, then allow, then ask, then the policy's default. */
export function decideFunction(policy: FunctionPolicy | undefined, name: string, registered: boolean): FunctionDecision {
  if (BUILTIN_FUNCTIONS.includes(name)) return `allow`
  if (!registered) return `not_found`
  const p = policy ?? {}
  const any = (list?: readonly string[]) => (list ?? []).some((pattern) => matchesPattern(pattern, name))
  if (any(p.deny)) return `deny`
  if (any(p.allow)) return `allow`
  if (any(p.ask)) return `ask`
  return p.default ?? `allow`
}

/** Two policies stacked (a package's allowlist under the host's own): the
 *  stricter decision wins. */
export function combineDecisions(a: FunctionDecision, b: FunctionDecision): FunctionDecision {
  const rank: Record<FunctionDecision, number> = { allow: 0, ask: 1, deny: 2, not_found: 3 }
  return rank[a] >= rank[b] ? a : b
}

/** A declarative package's `functions` list as a policy: listed = allow,
 *  anything else = deny. */
export function packagePolicy(functions: readonly string[] | undefined): FunctionPolicy {
  return { allow: functions ?? [], default: `deny` }
}

export interface UrlPolicy {
  schemes?: readonly string[]
  /** http(s) hosts allowed: exact or `*.example.com`; unset = any. */
  hosts?: readonly string[]
  baseUrl?: string
}

export interface UrlDecision {
  allowed: boolean
  /** The absolute url when it parses. */
  url?: string
  reason?: `invalid` | `scheme` | `host`
}

function absolute(url: string, baseUrl?: string): URL | null {
  try {
    return new URL(url)
  } catch {
    if (!baseUrl) return null
    try {
      return new URL(url, baseUrl)
    } catch {
      return null
    }
  }
}

function hostMatches(pattern: string, host: string): boolean {
  const p = pattern.toLowerCase()
  return p.startsWith(`*.`) ? host.endsWith(p.slice(1)) && host.length > p.length - 1 : host === p
}

export function decideUrl(policy: UrlPolicy | undefined, url: string): UrlDecision {
  const parsed = absolute(url.trim(), policy?.baseUrl)
  if (!parsed) return { allowed: false, reason: `invalid` }
  const scheme = parsed.protocol.replace(/:$/, ``).toLowerCase()
  const schemes = policy?.schemes ?? DEFAULT_URL_SCHEMES
  if (!schemes.includes(scheme)) return { allowed: false, url: parsed.href, reason: `scheme` }
  if (policy?.hosts && (scheme === `http` || scheme === `https`) && !policy.hosts.some((h) => hostMatches(h, parsed.hostname.toLowerCase())))
    return { allowed: false, url: parsed.href, reason: `host` }
  return { allowed: true, url: parsed.href }
}

export interface MediaRule {
  prefix: string
  headers: Record<string, string>
}

export interface MediaOptions {
  baseUrl?: string
  rules?: readonly MediaRule[]
}

export interface MediaRequest {
  url: string
  headers: Record<string, string>
}

/** The image / media loader's request: the absolute url plus the headers of
 *  every rule whose prefix it starts with (later rules win per header).
 *  Null when the url does not resolve. */
export function mediaRequest(url: string, options: MediaOptions = {}): MediaRequest | null {
  const parsed = absolute(url.trim(), options.baseUrl)
  if (!parsed) return null
  const href = parsed.href
  const headers: Record<string, string> = {}
  for (const rule of options.rules ?? []) if (href.startsWith(rule.prefix)) Object.assign(headers, rule.headers)
  return { url: href, headers: sortKeys(headers) }
}

export function sortKeys<T>(record: Record<string, T>): Record<string, T> {
  return Object.fromEntries(Object.entries(record).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)))
}
