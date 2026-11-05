// #EDE-8: the apply state machine of the eUI-on-disk runtime. Pure and
// byte-locked by fixtures/apply.json (its $comment is the spec); ede-ui
// (apps/ede/crates/ede-ui/src/apply.rs) replays the same cases in Rust.

import fixture from "../fixtures/apply.json" with { type: "json" }
import type { Issue } from "./surface"

export const APPLY_VERSION: number = fixture.version
export const RULES = fixture.rules as { debounceMs: number; frameAckMs: number; crashesToRevert: number }

export interface DocumentState {
  shown: string
  commits: string[]
  pending: { commit: string; deadline: number } | null
  crashes: number
  rejected: Issue[] | null
  reverted: boolean
  /** The newest commit holds the factory document (at start and after a `factory` effect): never a suspect. */
  fromFactory: boolean
}

export interface ApplyState {
  documents: Record<string, DocumentState>
  /** How many commits the reducer has named (`c<n>`). */
  commitCount: number
}

export type ApplyEvent =
  | { kind: `ui.edit`; at: number; data: { name: string; valid: boolean; issues?: Issue[] } }
  | { kind: `ui.frame`; at: number; data: { name: string; commit: string } }
  | { kind: `shell.crash`; at: number; data?: Record<string, never> }
  | { kind: `tick`; at: number; data?: Record<string, never> }

export type Effect =
  | { kind: `commit`; name: string; id: string }
  | { kind: `refuse`; name: string; issues: Issue[] }
  | { kind: `reload`; name: string; commit: string }
  | { kind: `revert`; name: string; from: string; to: string; id: string }
  | { kind: `factory`; name: string; id: string }

export const factoryCommit = (name: string): string => `f-${name}`

/** A fixture's `given.documents`: every field optional, the factory commit first. */
export function hydrate(given: { documents?: Record<string, Partial<DocumentState>> }): ApplyState {
  const documents: Record<string, DocumentState> = {}
  let commitCount = 0
  for (const [name, d] of Object.entries(given.documents ?? {})) {
    const commits = d.commits ?? [factoryCommit(name)]
    for (const c of commits) {
      const n = /^c(\d+)$/.exec(c)
      if (n) commitCount = Math.max(commitCount, Number(n[1]))
    }
    documents[name] = {
      shown: d.shown ?? commits[commits.length - 1],
      commits,
      pending: d.pending ?? null,
      crashes: d.crashes ?? 0,
      rejected: d.rejected ?? null,
      reverted: d.reverted ?? false,
      fromFactory: d.fromFactory ?? commits[commits.length - 1] === factoryCommit(name),
    }
  }
  return { documents, commitCount }
}

function clone(state: ApplyState): ApplyState {
  return { commitCount: state.commitCount, documents: Object.fromEntries(Object.entries(state.documents).map(([k, v]) => [k, { ...v, commits: [...v.commits], pending: v.pending ? { ...v.pending } : null, rejected: v.rejected ? [...v.rejected] : null }])) }
}

const names = (state: ApplyState): string[] => Object.keys(state.documents).sort()

function nextCommit(state: ApplyState): string {
  state.commitCount += 1
  return `c${state.commitCount}`
}

function revert(state: ApplyState, name: string, from: string, to: string, effects: Effect[]): void {
  const d = state.documents[name]
  const id = nextCommit(state)
  d.commits.push(id)
  d.shown = id
  d.pending = null
  d.crashes = 0
  d.reverted = true
  effects.push({ kind: `revert`, name, from, to, id }, { kind: `reload`, name, commit: id })
}

export function reduce(previous: ApplyState, event: ApplyEvent): { state: ApplyState; effects: Effect[] } {
  const state = clone(previous)
  const effects: Effect[] = []

  // Rule 0: a commit nobody drew in time rolls back.
  for (const name of names(state)) {
    const d = state.documents[name]
    if (d.pending && d.pending.deadline <= event.at) revert(state, name, d.pending.commit, d.shown, effects)
  }

  switch (event.kind) {
    case `ui.edit`: {
      const { name, valid } = event.data
      const d = state.documents[name]
      if (!d) {
        effects.push({ kind: `refuse`, name, issues: [{ path: `file`, message: `not a shell document (no factory copy): ${name}` }] })
        break
      }
      if (!valid) {
        const issues = event.data.issues ?? []
        d.rejected = issues
        effects.push({ kind: `refuse`, name, issues })
        break
      }
      const id = nextCommit(state)
      d.commits.push(id)
      d.pending = { commit: id, deadline: event.at + RULES.frameAckMs }
      d.crashes = 0
      d.reverted = false
      d.fromFactory = false
      d.rejected = null
      effects.push({ kind: `commit`, name, id }, { kind: `reload`, name, commit: id })
      break
    }
    case `ui.frame`: {
      const d = state.documents[event.data.name]
      if (d?.pending && d.pending.commit === event.data.commit) {
        d.shown = d.pending.commit
        d.pending = null
      }
      break
    }
    case `shell.crash`: {
      for (const name of names(state)) {
        const d = state.documents[name]
        if (d.fromFactory) continue
        const newest = d.commits[d.commits.length - 1]
        d.crashes += 1
        if (d.crashes < RULES.crashesToRevert) continue
        if (!d.reverted) {
          revert(state, name, newest, d.commits[d.commits.length - 2], effects)
        } else {
          const id = nextCommit(state)
          d.commits.push(id)
          d.shown = id
          d.pending = null
          d.crashes = 0
          d.reverted = false
          d.fromFactory = true
          effects.push({ kind: `factory`, name, id }, { kind: `reload`, name, commit: id })
        }
      }
      break
    }
    case `tick`:
      break
  }
  return { state, effects }
}
