// Action-input validation + resolution (EXP-257) — the pure core shared by
// the steer router (server-side value resolution with injected DB lookups)
// and the launch dialog (client-side required-field gating). No DB imports:
// lookups are injected so this stays unit-testable and client-bundle-safe.

import {
  MAX_ACTION_INPUT_TEXT,
  type ActionInputDef,
} from "@exp/db-schema/domain"

/** One FILLED action input, fully resolved server-side: `display` is the
 * human-readable form (repo fullName / board name / the text itself) so the
 * desktop can inject a readable "## Inputs" block with zero lookups, while
 * `value` keeps the raw id the agent must pass to MCP tools. */
export interface SteerStartInput {
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

export interface ActionInputLookups {
  /** null unless the repo exists AND belongs to teamId. */
  repo(id: string, teamId: string): Promise<{ fullName: string } | null>
  /** null unless the board exists in teamId and is not trashed. */
  board(id: string, teamId: string): Promise<{ name: string } | null>
  /** `pr` inputs (EXP-259): the value is an ISSUE id — null unless the issue
   * exists in teamId and carries an OPEN linked pull request. */
  pr(
    issueId: string,
    teamId: string
  ): Promise<{ identifier: string; prNumber: number | null } | null>
}

export type ResolveInputsResult =
  | { ok: true; inputs: SteerStartInput[] }
  | { ok: false; message: string }

/** Validate the raw client values against the action's input schema and
 * resolve repo/board ids to display names. Output is in DEFINITION order and
 * carries only filled keys. Runs even for an empty schema so unknown keys
 * still reject. */
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

  const inputs: SteerStartInput[] = []
  for (const def of defs) {
    const raw = values[def.key]
    const filled = raw !== undefined && raw.trim().length > 0
    if (!filled) {
      if (def.required) {
        return { ok: false, message: `Missing required input "${def.key}"` }
      }
      continue
    }

    if (def.type === `text`) {
      if (raw.length > MAX_ACTION_INPUT_TEXT) {
        return {
          ok: false,
          message: `Input "${def.key}" is too long (max ${MAX_ACTION_INPUT_TEXT} chars)`,
        }
      }
      if (raw.includes(`\u0000`)) {
        return { ok: false, message: `Input "${def.key}" contains NUL bytes` }
      }
      inputs.push({
        key: def.key,
        label: def.label,
        type: def.type,
        value: raw,
        display: raw,
      })
      continue
    }

    // repo / board / pr: the value is a picked id — must be a uuid that
    // resolves team-scoped, or the run is refused before waking the desktop.
    if (!UUID_RE.test(raw)) {
      return { ok: false, message: `Input "${def.key}" must be an id` }
    }
    if (def.type === `pr`) {
      const pr = await lookups.pr(raw, teamId)
      if (!pr) {
        return {
          ok: false,
          message: `Input "${def.key}": pick an open pull request of the team`,
        }
      }
      inputs.push({
        key: def.key,
        label: def.label,
        type: def.type,
        value: raw,
        display: pr.prNumber
          ? `#${pr.prNumber} · ${pr.identifier}`
          : pr.identifier,
      })
      continue
    }
    if (def.type === `repo`) {
      const repo = await lookups.repo(raw, teamId)
      if (!repo) {
        return {
          ok: false,
          message: `Input "${def.key}": repository must belong to the team`,
        }
      }
      inputs.push({
        key: def.key,
        label: def.label,
        type: def.type,
        value: raw,
        display: repo.fullName,
      })
      continue
    }
    const board = await lookups.board(raw, teamId)
    if (!board) {
      return {
        ok: false,
        message: `Input "${def.key}": board must belong to the team`,
      }
    }
    inputs.push({
      key: def.key,
      label: def.label,
      type: def.type,
      value: raw,
      display: board.name,
    })
  }
  return { ok: true, inputs }
}
