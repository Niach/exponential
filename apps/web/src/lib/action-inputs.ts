// Action-input validation + resolution (EXP-257) — the pure core shared by
// `sessions.send` / the brief (server-side value resolution with injected DB
// lookups) and the launch composer (client-side required-field gating). No
// DB imports: lookups are injected so this stays unit-testable and
// client-bundle-safe.

import type { ActionInputDef } from "@exp/db-schema/domain"

/** One FILLED action input, fully resolved server-side: `display` is the
 * human-readable form (repo fullName / board name / `#12 · title` / the
 * issue's identifier + title) the session brief lists, while `value` keeps
 * the raw id the agent passes to MCP tools. */
export interface ResolvedActionInput {
  key: string
  label: string
  type: string
  value: string
  display: string
}

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

// ── Client-side helpers ───────────────────────────────────────────────────────

/** Labels of required inputs the user hasn't filled (text: trimmed-empty
 * counts as missing). Drives the run button gate. */
export function missingRequiredInputs(
  defs: ActionInputDef[] | null | undefined,
  values: Record<string, string>
): string[] {
  if (!defs) return []
  return defs
    .filter((def) => def.required && !(values[def.key] ?? ``).trim())
    .map((def) => def.label)
}

/** The `inputs` record a client sends to steer.startSession: blank optionals
 * and unknown keys dropped; `undefined` when nothing to send. */
export function buildInputsPayload(
  defs: ActionInputDef[] | null | undefined,
  values: Record<string, string>
): Record<string, string> | undefined {
  if (!defs) return undefined
  const out: Record<string, string> = {}
  for (const def of defs) {
    const value = values[def.key]
    if (value !== undefined && value.trim().length > 0) out[def.key] = value
  }
  return Object.keys(out).length > 0 ? out : undefined
}

// ── Server-side resolution ────────────────────────────────────────────────────

// EXP-1291: every input is a team-scoped PICK (repo | board | pr | issue);
// the value is always an id.
export interface ActionInputLookups {
  /** null unless the repo exists AND belongs to teamId. */
  repo(id: string, teamId: string): Promise<{ fullName: string } | null>
  /** null unless the board exists in teamId and is not trashed/archived. */
  board(id: string, teamId: string): Promise<{ name: string } | null>
  /** `pr`: a `pull_requests` id — null unless it is an OPEN PR of teamId. */
  pr(id: string, teamId: string): Promise<{ number: number; title: string | null } | null>
  /** `issue`: an issue id — null unless it is in teamId on a visible board. */
  issue(id: string, teamId: string): Promise<{ identifier: string; title: string } | null>
}

export type ResolveInputsResult =
  | { ok: true; inputs: ResolvedActionInput[] }
  | { ok: false; message: string }

const refusals: Record<ActionInputDef[`type`], string> = {
  repo: `repository must belong to the team`,
  board: `board must belong to the team`,
  pr: `pick an open pull request of the team`,
  issue: `pick an issue of the team`,
}

async function displayOf(
  type: ActionInputDef[`type`],
  id: string,
  teamId: string,
  lookups: ActionInputLookups
): Promise<string | null> {
  switch (type) {
    case `repo`:
      return (await lookups.repo(id, teamId))?.fullName ?? null
    case `board`:
      return (await lookups.board(id, teamId))?.name ?? null
    case `pr`: {
      const pr = await lookups.pr(id, teamId)
      if (!pr) return null
      return pr.title ? `#${pr.number} · ${pr.title}` : `#${pr.number}`
    }
    case `issue`: {
      const issue = await lookups.issue(id, teamId)
      return issue ? `${issue.identifier} ${issue.title}` : null
    }
  }
}

/** Validate the raw values against the action's input schema and resolve
 * each id to its display. Output is in DEFINITION order and carries only
 * filled keys. Runs even for an empty schema so unknown keys still reject. */
export async function resolveActionInputs(
  defs: ActionInputDef[],
  values: Record<string, string>,
  teamId: string,
  lookups: ActionInputLookups
): Promise<ResolveInputsResult> {
  const known = new Set(defs.map((def) => def.key))
  for (const key of Object.keys(values)) {
    if (!known.has(key)) {
      return { ok: false, message: `Unknown input "${key}"` }
    }
  }

  const inputs: ResolvedActionInput[] = []
  for (const def of defs) {
    const raw = values[def.key]
    const filled = raw !== undefined && raw.trim().length > 0
    if (!filled) {
      if (def.required) {
        return { ok: false, message: `Missing required input "${def.key}"` }
      }
      continue
    }
    if (!UUID_RE.test(raw)) {
      return { ok: false, message: `Input "${def.key}" must be an id` }
    }
    const display = await displayOf(def.type, raw, teamId, lookups)
    if (display === null) {
      return { ok: false, message: `Input "${def.key}": ${refusals[def.type]}` }
    }
    inputs.push({ key: def.key, label: def.label, type: def.type, value: raw, display })
  }
  return { ok: true, inputs }
}
