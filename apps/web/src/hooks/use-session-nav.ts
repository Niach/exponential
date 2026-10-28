import { useCallback, useMemo } from "react"
import { useNavigate } from "@tanstack/react-router"
import type { UserAvatarAgent } from "@exp/ui"
import { agentCollection, sessionCollection } from "@/lib/collections"
import { useSession } from "@/hooks/use-session"
import { useAllAgentRows } from "@/hooks/use-agent-row"
import { backParam, parseBack, pushBack, sessionTarget } from "@/lib/session-back"
import type { SessionRow } from "@/lib/session-tree"

// S-3 inter-session links (web): `open` = an xref tap (opens that session,
// pushes the one on show onto the route's `back` stack), `pop` = the
// header's `‹` (absent while the stack is empty). `lib/session-back.ts`.

export function useSessionNav({
  teamSlug,
  here,
  back,
}: {
  teamSlug: string
  /** The session on show (pushed by `open`); null = none. */
  here: string | null
  /** The route's `back` search value. */
  back: string | undefined
}): {
  open: (sessionId: string) => void
  pop: (() => void) | undefined
  stack: string[]
} {
  const navigate = useNavigate()
  const { data: auth } = useSession()
  const viewerId = auth?.user?.id ?? null
  const stack = useMemo(() => parseBack(back), [back])
  const go = useCallback(
    (sessionId: string, next: readonly string[]) => {
      const target = sessionTarget(
        sessionId,
        (id) => sessionCollection.get(id) as SessionRow | undefined,
        viewerId
      )
      const backValue = backParam(next)
      const backSearch = backValue ? { back: backValue } : {}
      if (target.to === `thread`) {
        void navigate({
          to: `/t/$teamSlug/threads/$threadId`,
          params: { teamSlug, threadId: target.threadId },
          search: target.session
            ? { tab: `session`, session: target.session, ...backSearch }
            : backSearch,
        })
      } else if (target.to === `agent`) {
        void navigate({
          to: `/t/$teamSlug/agents/$agentId`,
          params: { teamSlug, agentId: target.agentId },
          search: backSearch,
        })
      } else {
        void navigate({
          to: `/t/$teamSlug/sessions/$sessionId`,
          params: { teamSlug, sessionId: target.sessionId },
          search: backSearch,
        })
      }
    },
    [navigate, teamSlug, viewerId]
  )
  const open = useCallback((sessionId: string) => go(sessionId, pushBack(stack, here)), [go, stack, here])
  const top = stack.at(-1)
  const pop = useMemo(
    () => (top ? () => go(top, stack.slice(0, -1)) : undefined),
    [go, stack, top]
  )
  return { open, pop, stack }
}

/** S-3: an xref's agent mark (its icon + colour) off the synced agents;
 *  null = not synced (the session glyph stands in). Stable per agents set. */
export function useXrefAgent(): (agentId: string) => UserAvatarAgent | null {
  const agents = useAllAgentRows()
  return useMemo(() => {
    const byId = new Map(agents.map((agent) => [agent.id, agent]))
    return (agentId: string) => {
      const agent = byId.get(agentId) ?? (agentCollection.get(agentId) as typeof agents[number] | undefined)
      return agent ? { icon: agent.icon ?? null, color: agent.color ?? null } : null
    }
  }, [agents])
}
