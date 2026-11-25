import { useEffect, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { SKIP_ERROR_TOAST } from "@/lib/action-error"
import { useSession } from "@/hooks/use-session"

// S-3 (agent-settings.json `assistant`): the viewer's Assistant is created
// ON FIRST USE: the composer home, the Settings pages and the chats list call
// `agents.ensureAssistant` when they mount. #EXP-1987 (agent-settings.json
// `assistant`): a screen WITH a current team sends it (a membership check
// only: the answer is the member's ONE Assistant) and is
// remembered per (account, team); the account Settings › Assistant page has
// no team and is remembered per account. Nothing REQUIRES the answer to sit
// in the current team. A created Assistant is remembered; "no device yet"
// (null) and a failure are not, so the next mount asks again.
const ensured = new Map<string, Promise<string | null>>()

export function ensureAssistantOnce(userId: string, teamId: string | null): Promise<string | null> {
  const key = `${userId}:${teamId ?? ``}`
  const pending = ensured.get(key)
  if (pending) return pending
  const next = trpc.agents.ensureAssistant
    .mutate(teamId ? { teamId } : {}, SKIP_ERROR_TOAST)
    .then((result) => {
      const id = result.agent?.id ?? null
      if (id === null) ensured.delete(key)
      return id
    })
    .catch((error: unknown) => {
      ensured.delete(key)
      throw error
    })
  ensured.set(key, next)
  return next
}

/** The viewer's Assistant id: undefined while asking (or `teamId`
 *  undefined = not yet known / not wanted), null when it can't exist yet (no
 *  device: `copy.noDevice`) or the call failed. `teamId` = the screen's
 *  current team; `null` = an account page with no team. */
export function useEnsureAssistant(teamId: string | null | undefined): string | null | undefined {
  const { data: authSession } = useSession()
  const userId = teamId === undefined ? null : (authSession?.user.id ?? null)
  const key = userId ? `${userId}:${teamId ?? ``}` : null
  const [state, setState] = useState<{ key: string; id: string | null } | null>(null)
  useEffect(() => {
    if (!userId || !key) return
    let live = true
    ensureAssistantOnce(userId, teamId ?? null)
      .then((id) => {
        if (live) setState({ key, id })
      })
      .catch(() => {
        if (live) setState({ key, id: null })
      })
    return () => {
      live = false
    }
  }, [userId, key, teamId])
  return state && state.key === key ? state.id : undefined
}
