// EXP-1490: one member's connection to one upstream (a team MCP server an
// app binds to). Pure and byte-locked by `fixtures/upstream.json` (its
// `$comment` is the spec).

import fixture from "../fixtures/upstream.json" with { type: "json" }

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
