import { useEffect, useMemo } from "react"
import { createFileRoute, redirect, useRouter } from "@tanstack/react-router"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { threadAskRow } from "@exp/domain-contract/session-tree"
import { PlanPage, SkeletonRows, useIsMobile } from "@exp/ui"
import { renderThreadText } from "@/components/thread/thread-markdown"
import { useAnswerMessage } from "@/hooks/use-answer-message"
import { useConversationSessions, useSessionRow, useSessionTitles } from "@/hooks/use-session-tree"
import { useSessionNav } from "@/hooks/use-session-nav"
import { messageCollection } from "@/lib/collections"
import { pageTitle } from "@/lib/page-title"
import { keyMessageRows, toTreeMessages, type SyncedMessageRow } from "@/lib/session-tree"

// #EXP-1493: THE plan page (fixtures/plan-page.json), deep-linkable:
// `/t/$teamSlug/sessions/$sessionId/plan/$messageId` (the `_` keeps it out
// of the session route, which redirects into its thread). The message is
// read by its root (#EXP-1380: messages are on demand); a message that is
// not a plan card opens the session instead.

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
  const router = useRouter()
  const mobile = useIsMobile()
  const { row: session, isReady } = useSessionRow(sessionId)
  const rootId = session ? (session.rootId ?? session.id) : null
  const { row: root } = useSessionRow(rootId)
  const { conversations } = useConversationSessions()
  const { data, isLoading } = useLiveQuery(
    (query) =>
      rootId
        ? query
            .from({ messages: messageCollection })
            .where(({ messages }) => and(eq(messages.rootId, rootId), eq(messages.id, messageId)))
        : undefined,
    [rootId, messageId]
  )
  const row = useMemo(() => {
    const keyed = keyMessageRows((data ?? []) as SyncedMessageRow[], conversations)
    const message = toTreeMessages(keyed)[0]
    const ask = message ? threadAskRow(message) : null
    return ask?.type === `plan` ? ask : null
  }, [data, conversations])
  const titled = useMemo(() => (root ? [root] : []), [root])
  const { titles } = useSessionTitles(titled)
  const chatName = (root ? (titles.get(root.id) ?? root.title) : null) ?? ``
  const { answer, pending } = useAnswerMessage()
  const nav = useSessionNav({ teamSlug, here: null, back: undefined })
  const openChat = () => nav.open(sessionId)
  const back = () => (window.history.length > 1 ? router.history.back() : openChat())

  const loading = (!session && !isReady) || (rootId !== null && isLoading && !row)
  // Not a plan card (or gone): the session it belongs to.
  const gone = !loading && !row && session !== undefined && session !== null
  useEffect(() => {
    if (gone) nav.open(sessionId)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [gone, sessionId])

  if (loading) {
    return (
      <div className="p-6">
        <SkeletonRows />
      </div>
    )
  }
  if (!row) return null
  return (
    <PlanPage
      row={row}
      chatName={chatName}
      mobile={mobile}
      renderText={renderThreadText}
      busy={pending.has(row.id)}
      onDecide={(input) => void answer(row.id, input)}
      onBack={back}
      onClose={mobile ? undefined : back}
      onOpenChat={openChat}
      className="min-h-0 flex-1"
    />
  )
}
