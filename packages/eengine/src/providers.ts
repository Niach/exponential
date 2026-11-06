// VAPP-260: what a host hands the Engine (`HostConfig`), the viewer each
// identity kind yields, and the network policy. Locked by `fixtures/providers.json`.

import { recordRefusal, type RecordOp, type RecordOwnership, type RecordPolicy, type RecordViewer } from "@exponential_app/eui/host"

import fixture from "../fixtures/providers.json" with { type: "json" }
import { capabilityNameValid } from "./capabilities"

export type ProviderKey = keyof typeof fixture.providers
export type NetworkPolicy = `off` | `upstreamsOnly` | `any`
export type IdentityKind = `member` | `local` | `peer`
export type AgentProviderId = `acp` | `localModel` | `remoteSession`

export const PROVIDER_KEYS = Object.keys(fixture.providers) as ProviderKey[]
export const FORBIDDEN_HOST_KEYS: readonly string[] = fixture.forbidden
export const NETWORK_POLICIES = fixture.networkPolicies as readonly NetworkPolicy[]
export const IDENTITY_KINDS = fixture.identityKinds as readonly IdentityKind[]
export const AGENT_PROVIDERS = fixture.agentProviders as readonly AgentProviderId[]

export interface HostConfigIssue {
  key: string
  code: `missing` | `forbidden` | `unknown` | `invalid` | `duplicateAgent` | `duplicateCapability` | `invalidCapability`
}

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)

/** Every problem with a host config: providers in order, then forbidden/unknown keys sorted. */
export function validateHostConfig(config: Record<string, unknown>): HostConfigIssue[] {
  const issues: HostConfigIssue[] = []
  for (const key of PROVIDER_KEYS) {
    const def = fixture.providers[key] as { required: boolean }
    const value = config[key]
    if (value === undefined) {
      if (def.required) issues.push({ key, code: `missing` })
      continue
    }
    if (key === `identity` && !(isObject(value) && (IDENTITY_KINDS as readonly unknown[]).includes(value.kind))) issues.push({ key, code: `invalid` })
    if (key === `network` && !(NETWORK_POLICIES as readonly unknown[]).includes(value)) issues.push({ key, code: `invalid` })
    if (key === `agents`) {
      const ids = Array.isArray(value) ? value.map((a) => (isObject(a) ? a.id : undefined)) : null
      if (!ids || ids.some((id) => !(AGENT_PROVIDERS as readonly unknown[]).includes(id))) issues.push({ key, code: `invalid` })
      else if (new Set(ids).size !== ids.length) issues.push({ key, code: `duplicateAgent` })
    }
    if (key === `capabilities`) {
      const names = Array.isArray(value) ? value.map((c) => (isObject(c) ? c.name : undefined)) : null
      if (!names || names.some((n) => typeof n !== `string` || !capabilityNameValid(n))) issues.push({ key, code: `invalidCapability` })
      else if (new Set(names).size !== names.length) issues.push({ key, code: `duplicateCapability` })
    }
  }
  const extra = Object.keys(config)
    .filter((k) => !(PROVIDER_KEYS as readonly string[]).includes(k))
    .sort()
  for (const key of extra) issues.push({ key, code: FORBIDDEN_HOST_KEYS.includes(key) ? `forbidden` : `unknown` })
  return issues
}

export type EngineIdentity =
  | { kind: `member`; userId: string; role: string | null; access?: `none` | `read` | `write` }
  | { kind: `local`; deviceId: string }
  | { kind: `peer`; key: string }

/** The records viewer of an identity: local = the one owner of everything ("me"). */
export function engineViewer(identity: EngineIdentity): Required<RecordViewer> {
  if (identity.kind === `local`) return { id: identity.deviceId, role: `owner`, access: `write` }
  if (identity.kind === `peer`) return { id: identity.key, role: `member`, access: `write` }
  return { id: identity.userId, role: identity.role, access: identity.access ?? `write` }
}

/** Why the viewer of `identity` may not `op` (null = allowed). */
export const engineRecordRefusal = (identity: EngineIdentity, policy: RecordPolicy, op: RecordOp, row?: RecordOwnership | null): string | null =>
  recordRefusal(policy, op, engineViewer(identity), row)

/** May the instance reach `target` under `policy`? `declared` = the package declares the upstream (or the origin). */
export function networkAllows(policy: NetworkPolicy, target: `upstream` | `sync` | `http`, declared: boolean): { allowed: boolean; reason: string | null } {
  if (policy === `off`) return { allowed: false, reason: `offline` }
  if (target === `upstream` && !declared) return { allowed: false, reason: `notDeclared` }
  if (target === `http` && policy === `upstreamsOnly`) return { allowed: false, reason: `network` }
  return { allowed: true, reason: null }
}
