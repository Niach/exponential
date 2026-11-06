// VAPP-260: the Engine's capability catalog and its ONE broker (app UI and
// agents alike). Pure and locked by `fixtures/capabilities.json` (its
// `$comment` is the spec); the `eengine` crate replays the same cases.

import { argsHash, hubOutcomeStatus, hubServerNameValid, type CallStatus, type HubOutcome } from "@exp/emcp"

import fixture from "../fixtures/capabilities.json" with { type: "json" }

export type ConsentKind = `ask` | `picker`
export type GrantValue = `granted` | `denied`
export type Grants = Record<string, GrantValue>
export type CallFrom = `ui` | `agent`

export interface CapabilityScope {
  id: string
  verb: string
  consent: ConsentKind
  tools: readonly string[]
  hostOnly?: boolean
  origins?: boolean
}

export interface CapabilityDef {
  name: string
  label: string
  scopes: readonly CapabilityScope[]
}

/** One entry of a package's capability manifest (format 5 `capabilities`). */
export interface ManifestEntry {
  id: string
  why: string
  origins?: readonly string[]
}

export const CAPABILITIES = fixture.capabilities as readonly CapabilityDef[]
export const RESERVED_CAPABILITY_NAMES: readonly string[] = fixture.reservedNames
const SCOPE = new RegExp(fixture.scopePattern)

/** Is `id` a scope id of the right form (`<capability>.<scope>`)? */
export const scopeIdValid = (id: string): boolean => SCOPE.test(id)

/** May a capability be mounted under `name`? A hub server name that is not reserved. */
export const capabilityNameValid = (name: string): boolean => hubServerNameValid(name) && !RESERVED_CAPABILITY_NAMES.includes(name)

export const capabilityDef = (name: string): CapabilityDef | null => CAPABILITIES.find((c) => c.name === name) ?? null

/** The catalog scope with `id`, with its capability. */
export function scopeDef(id: string): { capability: CapabilityDef; scope: CapabilityScope } | null {
  for (const capability of CAPABILITIES) {
    const scope = capability.scopes.find((s) => s.id === id)
    if (scope) return { capability, scope }
  }
  return null
}

const NAME_PART = /^[a-z][a-z0-9-]*$/

/** A call's name → `{capability, tool}`: `<capability>__<tool>` from an agent,
 *  `<capability>.<tool>` from app UI; null for any other form or a reserved name. */
export function parseCapabilityCall(from: CallFrom, name: string): { capability: string; tool: string } | null {
  const sep = from === `agent` ? `__` : `.`
  const at = name.indexOf(sep)
  if (at < 0) return null
  const capability = name.slice(0, at)
  const tool = name.slice(at + sep.length)
  if (!NAME_PART.test(capability) || !NAME_PART.test(tool)) return null
  if (RESERVED_CAPABILITY_NAMES.includes(capability)) return null
  return { capability, tool }
}

export type BrokerVerdictKind = `call` | `ask` | `denied` | `unknown` | `unavailable`

export interface BrokerVerdict {
  verdict: BrokerVerdictKind
  capability: string | null
  tool: string | null
  scope: string | null
  reason: string | null
}

export interface BrokerCall {
  from: CallFrom
  name: string
  arguments?: unknown
}

export interface BrokerInput {
  manifest: readonly ManifestEntry[]
  grants: Grants
  /** The capability names this host mounts. */
  provided: readonly string[]
  foreground: boolean
  call: BrokerCall
}

const UNKNOWN: BrokerVerdict = { verdict: `unknown`, capability: null, tool: null, scope: null, reason: `unknownTool` }

/** An `https://host[:port]` origin of a URL, or null. */
export function urlOrigin(url: unknown): string | null {
  if (typeof url !== `string`) return null
  try {
    const parsed = new URL(url)
    return parsed.origin === `null` ? null : parsed.origin
  } catch {
    return null
  }
}

/** The broker's verdict for one call (capabilities.json `decideCases`). */
export function brokerDecide(input: BrokerInput): BrokerVerdict {
  const parsed = parseCapabilityCall(input.call.from, input.call.name)
  if (!parsed) return UNKNOWN
  const capability = capabilityDef(parsed.capability)
  const scope = capability?.scopes.find((s) => s.tools.includes(parsed.tool))
  if (!capability || !scope) return UNKNOWN
  const at = (verdict: BrokerVerdictKind, reason: string | null): BrokerVerdict => ({ verdict, capability: capability.name, tool: parsed.tool, scope: scope.id, reason })
  if (scope.hostOnly) return at(`denied`, `hostOnly`)
  const entry = input.manifest.find((m) => m.id === scope.id)
  if (!entry) return at(`denied`, `notDeclared`)
  if (!input.provided.includes(capability.name)) return at(`unavailable`, `unavailable`)
  if (scope.origins) {
    const args = input.call.arguments
    const origin = urlOrigin(args && typeof args === `object` ? (args as Record<string, unknown>).url : undefined)
    if (!origin || !(entry.origins ?? []).includes(origin)) return at(`denied`, `originNotDeclared`)
  }
  if (scope.consent === `picker`) return input.foreground ? at(`call`, null) : at(`denied`, `needsForeground`)
  const grant = input.grants[scope.id]
  if (grant === `granted`) return at(`call`, null)
  if (grant === `denied`) return at(`denied`, `consentDenied`)
  return input.foreground ? at(`ask`, null) : at(`denied`, `consentNeeded`)
}

/** An agent's tools/list: every tool of a declared, provided, non-host-only scope. */
export function agentToolList(manifest: readonly ManifestEntry[], provided: readonly string[]): string[] {
  const names: string[] = []
  for (const capability of CAPABILITIES) {
    if (!provided.includes(capability.name)) continue
    for (const scope of capability.scopes) {
      if (scope.hostOnly || !manifest.some((m) => m.id === scope.id)) continue
      for (const tool of scope.tools) names.push(`${capability.name}__${tool}`)
    }
  }
  return names.sort()
}

export interface BrokerAuditRow {
  appId: string
  clientId: string
  tool: string
  status: CallStatus
  error: string | null
  argsHash: string
}

/** The audit row of one brokered call. `client` = the agent run's id (null for UI);
 *  `outcome` = the call's outcome when it ran (`verdict: call`), else ignored. */
export function brokerAuditRow(input: { appId: string; client: string | null; call: BrokerCall; verdict: BrokerVerdict; outcome: HubOutcome | null }): BrokerAuditRow {
  const { appId, client, call, verdict } = input
  const tool = verdict.capability && verdict.tool ? `${verdict.capability}__${verdict.tool}` : call.from === `ui` ? call.name.replace(`.`, `__`) : call.name
  const base = { appId, clientId: call.from === `ui` ? `ui` : (client ?? ``), tool, argsHash: argsHash(call.arguments ?? {}) }
  if (verdict.verdict === `call`) {
    const { status, error } = hubOutcomeStatus(input.outcome ?? `ok`)
    return { ...base, status, error }
  }
  return { ...base, status: `denied`, error: verdict.reason }
}
