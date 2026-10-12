import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"

// Final review #8/B7 (feedback-states.json `autosave`): the team prompt's
// saves, SERIALIZED per team. One save in flight per team; an edit typed
// meanwhile waits as the team's queued text and the LAST one wins. A failed
// save toasts with a Retry bound to THAT team, which resends the team's
// CURRENT text (the mounted editor's draft, else the newest text queued for
// it), never the snapshot that failed. The editor's debounce, blur and
// unmount/team-switch flush all go through `queueTeamPromptSave`, so a flush
// can never race an in-flight save. Module-level on purpose: a flush or a
// Retry outlives the editor that started it.

export type TeamPromptSaveEvent =
  | { kind: `saving`; teamId: string; text: string }
  | { kind: `saved`; teamId: string; text: string }
  | { kind: `failed`; teamId: string; text: string }

interface TeamPromptQueue {
  inFlight: string | null
  queued: string | null
  /** The newest text handed to the queue (what a Retry falls back to). */
  latest: string
}

const queues = new Map<string, TeamPromptQueue>()
const listeners = new Set<(event: TeamPromptSaveEvent) => void>()
// The mounted editor's live draft per team (null = not loaded for it).
const drafts = new Map<string, () => string | null>()

function emit(event: TeamPromptSaveEvent): void {
  for (const listener of listeners) listener(event)
}

/** Saves `text` as the team's prompt: at once when the team has no save in
 *  flight, else after it (the newest queued text wins). */
export function queueTeamPromptSave(teamId: string, text: string): void {
  let queue = queues.get(teamId)
  if (!queue) {
    queue = { inFlight: null, queued: null, latest: text }
    queues.set(teamId, queue)
  }
  queue.latest = text
  if (queue.inFlight !== null) {
    // The text already on its way needs no second trip.
    queue.queued = text === queue.inFlight ? null : text
    return
  }
  void run(teamId, queue, text)
}

async function run(
  teamId: string,
  queue: TeamPromptQueue,
  text: string
): Promise<void> {
  queue.inFlight = text
  emit({ kind: `saving`, teamId, text })
  try {
    await trpc.teams.update.mutate({ teamId, agentPrompt: text }, SKIP_ERROR_TOAST)
    emit({ kind: `saved`, teamId, text })
  } catch (error) {
    // A newer edit queued behind it supersedes the failed text: it goes out
    // next and toasts only if it fails too.
    if (queue.queued === null) {
      emit({ kind: `failed`, teamId, text })
      failedAction(`save`, `the team prompt`, error, {
        retry: () => retryTeamPromptSave(teamId),
      })
    }
  } finally {
    queue.inFlight = null
    const next = queue.queued
    queue.queued = null
    if (next !== null) void run(teamId, queue, next)
  }
}

/** A failed save's Retry: the team's CURRENT text, through the queue. */
export function retryTeamPromptSave(teamId: string): void {
  const queue = queues.get(teamId)
  const current = drafts.get(teamId)?.() ?? queue?.latest
  if (current === undefined) return
  queueTeamPromptSave(teamId, current)
}

/** True while a save for the team is on its way. */
export function teamPromptSaving(teamId: string): boolean {
  return (queues.get(teamId)?.inFlight ?? null) !== null
}

/** The editor reports its live draft for `teamId` (Retry reads it). */
export function registerTeamPromptDraft(
  teamId: string,
  current: () => string | null
): () => void {
  drafts.set(teamId, current)
  return () => {
    if (drafts.get(teamId) === current) drafts.delete(teamId)
  }
}

export function onTeamPromptSave(
  listener: (event: TeamPromptSaveEvent) => void
): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

// Test hook.
export function _resetTeamPromptSaves(): void {
  queues.clear()
  drafts.clear()
}
