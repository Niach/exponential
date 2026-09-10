// Action-input validation + resolution (EXP-257) — the pure core shared by
// the steer router (server-side value resolution with injected DB lookups)
// and the launch composer (client-side required-field gating). No DB imports:
// lookups are injected so this stays unit-testable and client-bundle-safe.

import { z } from "zod"
import {
  actionInputDefSchema,
  actionInputTypeValues,
  boardIconValues,
  MAX_ACTION_INPUTS,
  MAX_ACTION_PROMPT_PLACEHOLDER,
  type ActionInputDef,
} from "@exp/db-schema/domain"

// ── EXP-825 compat: the retired free-text input kinds ─────────────────────────
// EXP-825 compat: action editors below the EXP-825 floor (iOS ≤ 0.14.28,
// Android ≤ 0.14.30, desktop ≤ 0.14.35 and its creator run telling the agent
// `type: text`) still submit `text`/`textarea` input definitions. The
// boundary accepts them and DROPS them, seeding the composer hint
// (`promptPlaceholder`) from the FIRST dropped definition's placeholder (else
// its label, LEFT 200) when the row has none yet — exactly what migration
// 0108 did to the stored rows. The STORED shape never carries these kinds.
// Remove when ios min >= 0.14.29, android min >= 0.14.31, desktop/cli min
// >= 0.14.36 (then `actionInputsSchema` goes back on the routers and the MCP
// tools).
export const RETIRED_ACTION_INPUT_TYPES = [`text`, `textarea`] as const

export const compatActionInputDefSchema = actionInputDefSchema.extend({
  type: z.enum([...actionInputTypeValues, ...RETIRED_ACTION_INPUT_TYPES]),
})
export type CompatActionInputDef = z.infer<typeof compatActionInputDefSchema>

/** `actionInputsSchema` widened to the retired kinds — the write boundary
 * only; normalize with `retireLegacyActionInputs` before storing. */
export const compatActionInputsSchema = z
  .array(compatActionInputDefSchema)
  .max(MAX_ACTION_INPUTS)
  .superRefine((defs, ctx) => {
    const seen = new Set<string>()
    for (const def of defs) {
      if (seen.has(def.key)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: `Duplicate input key "${def.key}"`,
        })
      }
      seen.add(def.key)
    }
  })

export interface RetiredActionInputs {
  /** The definitions to store — pick kinds only, in order; `undefined`
   * when none were submitted. */
  inputs: ActionInputDef[] | undefined
  /** The composer hint to seed, ONLY when a retired definition was dropped
   * and `currentPromptPlaceholder` is blank; `undefined` = leave it alone. */
  promptPlaceholder: string | undefined
}

/** Drop the retired `text`/`textarea` definitions from a submitted input
 * schema (migration 0108's rule, applied at the write boundary). */
export function retireLegacyActionInputs(
  defs: CompatActionInputDef[] | undefined,
  currentPromptPlaceholder: string | null | undefined
): RetiredActionInputs {
  if (!defs) return { inputs: undefined, promptPlaceholder: undefined }
  const retired = (RETIRED_ACTION_INPUT_TYPES as readonly string[]).slice()
  const dropped = defs.filter((def) => retired.includes(def.type))
  const inputs = defs.filter(
    (def) => !retired.includes(def.type)
  ) as ActionInputDef[]
  const first = dropped[0]
  const seed =
    first && !(currentPromptPlaceholder ?? ``).trim()
      ? (first.placeholder?.trim() || first.label)
          .slice(0, MAX_ACTION_PROMPT_PLACEHOLDER)
          .trim()
      : undefined
  return { inputs, promptPlaceholder: seed || undefined }
}

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

    // icon (EXP-273): the only non-id picked value — a curated registry name,
    // validated against the contract set rather than a team-scoped lookup
    // (icons are global, so there is nothing to scope). Checked before the
    // uuid gate below, which it would otherwise fail.
    if (def.type === `icon`) {
      if (!(boardIconValues as readonly string[]).includes(raw)) {
        return {
          ok: false,
          message: `Input "${def.key}": pick an icon from the curated set`,
        }
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
