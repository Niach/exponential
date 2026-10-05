import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import type { RunMarkState } from "@exp/ui"
import { attachmentIdFromUrl } from "./actions"
import type { RunDetail, RunResult } from "./model"

// EXP-1183 — the pure half of the run face: what `exponential_sessions_get`,
// `exponential_issues_pr_files`, `exponential_sessions_message` and
// `exponential_sessions_start` answer, folded into what the view draws.

/** The run fields the run face reads beyond the shared `RunDetail`. */
export interface RunFace extends RunDetail {
  deviceId?: string | null
  endedBy?: string | null
}

/** How often a LIVE run re-reads itself (state, caption, new results). */
export const RUN_POLL_MS = 15_000

/** Live = steerable: running, or its PR open and the run still up. */
export function runIsLive(run: Pick<RunDetail, `status`>): boolean {
  return run.status === `running` || run.status === `in_review`
}

/** The run's mark badge (AgentRunMark): it waits on you, it works, its PR is
 *  open; an idle or ended run wears the bare mark. */
export function runMarkState(run: RunDetail): RunMarkState | undefined {
  if (!runIsLive(run)) return undefined
  if (run.needsInput) return `needs_input`
  if (run.agentBusy) return `working`
  if (run.status === `in_review`) return `review`
  return undefined
}

/** The state pill's word: the caption rides its own line, not the pill. */
export function runPillLabel(run: RunDetail): string {
  if (run.status === `ended`) return `Ended`
  if (run.needsInput) return `Needs input`
  if (run.agentBusy) return `Working`
  if (run.status === `in_review`) return `In review`
  return `Idle`
}

/** The working caption, only while the agent works. */
export function runWorkingCaption(run: RunDetail): string | null {
  if (!runIsLive(run) || !run.agentBusy || run.needsInput) return null
  return run.agentCaption?.trim() || null
}

/** Every distinct attachment id the run's pictures point at, in order. */
export function runAttachmentIds(results: readonly RunResult[] | null | undefined): string[] {
  const ids: string[] = []
  for (const result of results ?? []) {
    const id = attachmentIdFromUrl(result.url)
    if (id && !ids.includes(id)) ids.push(id)
  }
  return ids
}

/**
 * `sessions_get` results → the records `parseSessionResultGroups` reads: text
 * entries as `{topic, text, files}`, pictures as `{topic, label,
 * attachmentId}` (the label falls back to the topic: the tool omits an unset
 * one, the parser drops a label-less picture). With `signed`, a picture whose
 * URL did not resolve (yet) is left out rather than drawn broken.
 */
export function runResultRecords(
  results: readonly RunResult[] | null | undefined,
  signed?: Readonly<Record<string, string | null>>
): Record<string, unknown>[] {
  const out: Record<string, unknown>[] = []
  for (const result of results ?? []) {
    if (typeof result.text === `string`) {
      out.push({ topic: result.topic, text: result.text, files: result.files ?? [] })
      continue
    }
    const attachmentId = attachmentIdFromUrl(result.url)
    if (!attachmentId) continue
    if (signed && !signed[attachmentId]) continue
    out.push({
      topic: result.topic,
      label: result.label?.trim() || result.topic,
      attachmentId,
    })
  }
  return out
}

interface PullFileRow {
  filename: string
  previous_filename?: string | null
  status: string
  additions: number
  deletions: number
  patch?: string | null
}

/** `exponential_issues_pr_files` → the shared diff model (empty on junk). */
export function diffFromPrFiles(payload: unknown): DiffFile[] {
  const files = (payload as { files?: unknown } | null)?.files
  if (!Array.isArray(files)) return []
  return files
    .filter(
      (file): file is PullFileRow =>
        !!file && typeof (file as PullFileRow).filename === `string`
    )
    .map((file) =>
      fromPullFile({
        ...file,
        additions: Number(file.additions) || 0,
        deletions: Number(file.deletions) || 0,
      })
    )
}

/** What a steer message's answer says back under the composer. */
export function steerFeedback(answer: unknown): string {
  const data = (answer ?? {}) as {
    queued?: boolean
    consumed?: boolean
    delivered?: boolean
  }
  if (data.queued) return `Queued: the agent reads it after this step.`
  if (data.consumed) return `Sent.`
  if (data.delivered && data.consumed === false) {
    return `Delivered, but the agent has not picked it up.`
  }
  return `Sent.`
}

/** The new run a Resume started (`sessions_start` answers `sessionId`). */
export function resumedRunId(answer: unknown): string | null {
  const data = (answer ?? {}) as {
    sessionId?: unknown
    session?: { id?: unknown } | null
  }
  if (typeof data.sessionId === `string` && data.sessionId) return data.sessionId
  if (typeof data.session?.id === `string` && data.session.id) return data.session.id
  return null
}
