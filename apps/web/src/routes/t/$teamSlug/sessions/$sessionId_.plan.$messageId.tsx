import { useEffect } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { SkeletonRows } from "@exp/ui"
import { useSessionRow } from "@/hooks/use-session-tree"
import { useSessionNav } from "@/hooks/use-session-nav"
import { pageTitle } from "@/lib/page-title"

// #EXP-1493 → #EXP-1662 (fixtures/plan-page.json): the plan's deep link,
// `/t/$teamSlug/sessions/$sessionId/plan/$messageId` (the `_` keeps it out of
// the session route). The plan is a tab of its chat: this route only finds
// that chat (an agent's conversation, or the root's chat) and replaces
// itself with the tab.

export const Route = createFileRoute(`/t/$teamSlug/sessions/$sessionId_/plan/$messageId`)({
  head: () => ({ meta: [{ title: pageTitle(`Plan`) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({ to: `/auth/login`, search: { redirect: location.href } })
    }
  },
  component: PlanRoute,
})

function PlanRoute() {
  const { teamSlug, sessionId, messageId } = Route.useParams()
  const navigate = useNavigate()
  const { row: session, isReady } = useSessionRow(sessionId)
  const rootId = session ? (session.rootId ?? session.id) : null
  const { row: root } = useSessionRow(rootId)
  const nav = useSessionNav({ teamSlug, here: null, back: undefined })
  useEffect(() => {
    if (!isReady && !session) return
    if (!session) {
      nav.open(sessionId)
      return
    }
    if (!root || !rootId) return
    const search = { open: `plan` as const, plan: messageId }
    if (root.agentId) {
      void navigate({ to: `/t/$teamSlug/agents/$agentId`, params: { teamSlug, agentId: root.agentId }, search, replace: true })
    } else {
      void navigate({ to: `/t/$teamSlug/sessions/$sessionId`, params: { teamSlug, sessionId: rootId }, search, replace: true })
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isReady, session?.id, root?.id, messageId])
  return (
    <div className="p-6">
      <SkeletonRows />
    </div>
  )
}
