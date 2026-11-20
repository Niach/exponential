import { useEffect, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { SKIP_ERROR_TOAST } from "@/lib/action-error"
import { useSession } from "@/hooks/use-session"

// S-3 (agent-settings.json `assistant`): the viewer's Assistant is created
// ON FIRST USE: the composer home, the Settings pages and the chats list call
// `agents.ensureAssistant` when they mount. #EXP-1987: WITHOUT a team, once
// per ACCOUNT per app run (the Assistant is the member's, not a team's: after
// the move it lives in their personal team). A created Assistant is
// remembered; "no device yet" (null) and a failure are not, so the next
// mount asks again.
const ensured = new Map<string, Promise<string | null>>()

export function ensureAssistantOnce(userId: string): Promise<string | null> {
  const pending = ensured.get(userId)
  if (pending) return pending
  const next = trpc.agents.ensureAssistant
    .mutate({}, SKIP_ERROR_TOAST)
    .then((result) => {
      const id = result.agent?.id ?? null
      if (id === null) ensured.delete(userId)
      return id
    })
    .catch((error: unknown) => {
      ensured.delete(userId)
      throw error
    })
  ensured.set(userId, next)
  return next
}

/** The viewer's Assistant id: undefined while asking (or `enabled` false),
 *  null when it can't exist yet (no device: `copy.noDevice`) or the call
 *  failed. Independent of the current team. */
export function useEnsureAssistant(enabled = true): string | null | undefined {
  const { data: authSession } = useSession()
  const userId = enabled ? (authSession?.user.id ?? null) : null
  const [state, setState] = useState<{ userId: string; id: string | null } | null>(null)
  useEffect(() => {
    if (!userId) return
    let live = true
    ensureAssistantOnce(userId)
      .then((id) => {
        if (live) setState({ userId, id })
      })
      .catch(() => {
        if (live) setState({ userId, id: null })
      })
    return () => {
      live = false
    }
  }, [userId])
  return state && state.userId === userId ? state.id : undefined
}
