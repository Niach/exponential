import { useEffect, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { SKIP_ERROR_TOAST } from "@/lib/action-error"

// S-3 (agent-settings.json `assistant`): the viewer's Assistant is created
// ON FIRST USE: the composer home, the Settings Agents group and the chats
// list call `agents.ensureAssistant` when they mount, once per team per app
// run. A created Assistant is remembered; "no device yet" (null) and a
// failure are not, so the next mount asks again.
const ensured = new Map<string, Promise<string | null>>()

export function ensureAssistantOnce(teamId: string): Promise<string | null> {
  const pending = ensured.get(teamId)
  if (pending) return pending
  const next = trpc.agents.ensureAssistant
    .mutate({ teamId }, SKIP_ERROR_TOAST)
    .then((result) => {
      const id = result.agent?.id ?? null
      if (id === null) ensured.delete(teamId)
      return id
    })
    .catch((error: unknown) => {
      ensured.delete(teamId)
      throw error
    })
  ensured.set(teamId, next)
  return next
}

/** The viewer's Assistant id in `teamId`: undefined while asking, null when
 *  it can't exist yet (no device: `copy.noDevice`) or the call failed. */
export function useEnsureAssistant(teamId: string | null | undefined): string | null | undefined {
  const [state, setState] = useState<{ teamId: string; id: string | null } | null>(null)
  useEffect(() => {
    if (!teamId) return
    let live = true
    ensureAssistantOnce(teamId)
      .then((id) => {
        if (live) setState({ teamId, id })
      })
      .catch(() => {
        if (live) setState({ teamId, id: null })
      })
    return () => {
      live = false
    }
  }, [teamId])
  return state && state.teamId === teamId ? state.id : undefined
}
