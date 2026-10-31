// EXP-850 §5: what the transcript's working row SAYS while the agent runs —
// `Pondering… (2m 04s · ↓ 12.4k tokens)`, or the running workflow's caption
// with the same suffix. Pure and byte-locked ×4 (desktop `steer`, iOS
// `WorkingCaption.swift`, Android `WorkingCaption.kt`); the verb ladder, the
// token tick and the preview cap are contract constants, and the workflow
// caption itself is the SHARED implementation in @exp/domain-contract —
// never a second copy of those rules.
import {
  contract,
  workflowCaption,
  type WorkflowCaptionInput,
} from "@exp/domain-contract"
import fixture from "@exp/domain-contract/fixtures/working-caption.json" with { type: "json" }

/** The verbs a turn cycles through, in contract order. */
export const WORKING_VERBS: readonly string[] = contract.steerWorking.verbs

/** The fallback when the publisher named no turn start (codex, and every
 *  pre-EXP-850 desktop) — no verb to pick and no numbers to show. */
export const WORKING_FALLBACK = fixture.copy.fallback

/** EXP-1501: the agent is STREAMING one tool call's input (the `turn` slot's
 *  `writing`): the tool it is writing and when that stream began. */
export interface WorkingWriting {
  tool: string
  startedAt: number
}

/** EXP-1501: the tool name the writing caption shows — whitespace collapsed,
 *  an `mcp__<server>__` prefix dropped; null = blank. */
export function writingToolLabel(tool: string): string | null {
  const clean = tool.replace(/\s+/g, ` `).trim()
  const split = clean.startsWith(`mcp__`) ? clean.indexOf(`__`, 5) : -1
  const label = split < 0 ? clean : clean.slice(split + 2)
  return label || null
}

/** The verb this turn wears: `verbs[startedAt % verbs.length]`, so every
 *  client watching the same run reads the same word, and a resumed viewer
 *  does not reshuffle it. */
export function workingVerb(startedAt: number): string {
  const verbs = WORKING_VERBS
  if (verbs.length === 0) return WORKING_FALLBACK
  // `startedAt` is an epoch in ms; a negative or fractional one still has to
  // land on a real index.
  const index = Math.abs(Math.trunc(startedAt)) % verbs.length
  return verbs[index]
}

/** `37s` / `2m 04s` / `1h 03m` — the same ladder ×4. Sub-second and negative
 *  durations read as `0s` rather than disappearing. */
export function formatTurnDuration(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000))
  if (total < 60) return `${total}s`
  const minutes = Math.floor(total / 60)
  if (minutes < 60) return `${minutes}m ${String(total % 60).padStart(2, `0`)}s`
  const hours = Math.floor(minutes / 60)
  return `${hours}h ${String(minutes % 60).padStart(2, `0`)}m`
}

/** `812` / `2.0k` / `1.2M` — one decimal from a thousand up, truncated (never
 *  rounded up past a threshold the agent has not reached). */
export function formatTokenCount(tokens: number): string {
  const count = Math.max(0, Math.floor(tokens))
  if (count < 1_000) return String(count)
  if (count < 1_000_000) return `${(Math.floor(count / 100) / 10).toFixed(1)}k`
  return `${(Math.floor(count / 100_000) / 10).toFixed(1)}M`
}

export interface WorkingCaptionInput {
  /** The turn's start (ms epoch) — picks the verb and measures the duration.
   *  Absent = the fallback caption with no suffix. */
  startedAt?: number | null
  /** Now (ms epoch), so the caption is a pure function of its inputs. */
  now?: number | null
  /** Output tokens this turn produced so far. */
  tokens?: number | null
  /** §7: while a workflow runs, the caption text IS its caption. */
  workflow?: WorkflowCaptionInput | null
  /** EXP-1501: a tool call's input is streaming — `Writing {tool}…`, timed
   *  from the stream's own start. */
  writing?: WorkingWriting | null
}

/** The whole line: `{verb}… ({duration} · ↓ {tokens} tokens)`. The
 *  duration/token group is omitted while unknown, a running workflow
 *  replaces the verb with `workflowCaption(w)` (the shared implementation),
 *  and a streaming tool call with `Writing {tool}…` on its own clock
 *  (`fixtures/working-caption.json`). */
export function workingCaption(input: WorkingCaptionInput): string {
  const writing =
    input.writing && Number.isFinite(input.writing.startedAt) ? input.writing : null
  const turnStartedAt =
    typeof input.startedAt === `number` && Number.isFinite(input.startedAt)
      ? input.startedAt
      : null
  const startedAt = writing ? writing.startedAt : turnStartedAt
  const writingLabel = writing ? writingToolLabel(writing.tool) : null
  const head = input.workflow
    ? workflowCaption(input.workflow)
    : writing
      ? writingLabel
        ? fixture.copy.writing.replace(`{tool}`, writingLabel)
        : fixture.copy.writingBlank
      : turnStartedAt === null
        ? WORKING_FALLBACK
        : `${workingVerb(turnStartedAt)}…`
  const parts: string[] = []
  if (
    startedAt !== null &&
    typeof input.now === `number` &&
    Number.isFinite(input.now)
  ) {
    parts.push(formatTurnDuration(input.now - startedAt))
  }
  if (
    typeof input.tokens === `number` &&
    Number.isFinite(input.tokens) &&
    input.tokens > 0
  ) {
    parts.push(`↓ ${formatTokenCount(input.tokens)} tokens`)
  }
  return parts.length === 0 ? head : `${head} (${parts.join(` · `)})`
}
