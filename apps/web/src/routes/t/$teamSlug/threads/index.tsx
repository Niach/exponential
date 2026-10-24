import { useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import type { ChatFilter } from "@exp/domain-contract/chats"
import { AgentBubbles, ChatFilterChips, ChatList, ChatsHeader, useIsMobile } from "@exp/ui"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useChatList } from "@/hooks/use-chat-list"
import { useEnteringRows } from "@/hooks/use-entering-rows"
import { useIssueSearch } from "@/hooks/use-issue-search"
import { useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import { pageTitle } from "@/lib/page-title"
import { THREADS_LIST_COPY } from "@/lib/session-tree"

// Thread 58cb32df + #EXP-1380 (chats.json LIST + WIDE): the product calls
// them CHATS, and every form factor paints the SAME list of the ONE active
// team: the agent bubbles, the filter chips, the flat rows (right-click /
// long-press = the row's menu through `menuProps`). Phones put the large
// headline (search + inbox) on top; wide keeps the list in one column at
// the page's list width (the old Needs you · Running · Recent bands left
// the page; the rail keeps its own rows).

export const Route = createFileRoute(`/t/$teamSlug/threads/`)({
  head: () => ({ meta: [{ title: pageTitle(THREADS_LIST_COPY.title) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: ThreadsPage,
})

function ThreadsPage() {
  const isMobile = useIsMobile()
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const { data: authSession } = useSession()
  const [filter, setFilter] = useState<ChatFilter>(`all`)
  const { list, bubbles, cliOf, isReady } = useChatList(team?.id, authSession?.user?.id, filter)
  const entering = useEnteringRows(list.rows.map((row) => row.sessionId))
  const unread = useUnreadNotificationCount(team?.id)
  const issueSearch = useIssueSearch()

  return (
    // EXP-771: the SCROLLER is full width, the reading column lives inside it.
    <div className="h-full overflow-y-auto">
      <div
        className={`mx-auto flex w-full max-w-3xl flex-col gap-4 px-4 ${
          isMobile ? `pt-[max(1rem,env(safe-area-inset-top))]` : `pt-4 pb-4`
        } ${TAB_BAR_CLEARANCE}`}
      >
        {isMobile ? (
          <ChatsHeader
            inboxUnread={unread > 0}
            onSearch={issueSearch.open}
            onInbox={() => void navigate({ to: `/t/$teamSlug/inbox`, params: { teamSlug } })}
          />
        ) : null}
        <AgentBubbles
          bubbles={bubbles}
          onOpen={(agentId) =>
            void navigate({ to: `/t/$teamSlug/agents/$agentId`, params: { teamSlug, agentId } })
          }
        />
        <ChatFilterChips value={filter} counts={list.counts} onChange={setFilter} />
        <ChatList
          rows={list.rows}
          agentOf={cliOf}
          loading={!team || !isReady}
          entering={entering}
          onOpen={(threadId) =>
            void navigate({ to: `/t/$teamSlug/threads/$threadId`, params: { teamSlug, threadId } })
          }
        />
      </div>
    </div>
  )
}
