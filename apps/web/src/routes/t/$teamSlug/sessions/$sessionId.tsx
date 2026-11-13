import { useEffect, useState } from "react"
import { createFileRoute, Link, Navigate, redirect, useNavigate } from "@tanstack/react-router"
import { Button, SkeletonRows } from "@exp/ui"
import { Empty } from "@/components/empty"
import { ChatScreen, validateChatSearch, type ChatSearch } from "@/components/chat/chat-screen"
import { SessionScreen } from "@/components/chat/session-body"
import { emptyStateCopy } from "@/lib/empty-states"
import { useSessionChat, useSessionRow } from "@/hooks/use-session-tree"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useSession } from "@/hooks/use-session"
import { useApps } from "@/hooks/use-apps"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import { CHATS_LIST_COPY, type SessionRow } from "@/lib/session-tree"

// EXP-1733: ONE route per session. A chat IS a root session, so
// `/sessions/$id` of a ROOT renders THE session screen (#EXP-1732,
// session-screen.json ×4, `components/chat/chat-screen.tsx`): the header
// (the lead disc, the title over ONE subtitle line, the merge pill, the
// `…`) and THE SegmentedControl Chat · Issues · Sessions · Results; wide =
// the chat beside the tab's pane. The search:
//
//   tab=chat|issues|sessions|results   the active tab (legacy ids map
//                                      through `legacyTabs`)
//   open=issue&issue=<id>              that issue's body, IN PLACE
//   open=session&session=<id>          EXP-1316: that session's RAW WORK in
//                                      place (its composer under it)
//   open=plan&plan=<id>                the plan page (plan-page.json)
//   tab=results[&section=&file=]       the Results body / its review pane
//
// A CHILD's `/sessions/$id` REDIRECTS (replace) to its root with
// `?tab=sessions&open=session&session=<id>`, keeping `from` and `back`. A
// session under a team AGENT keeps its own screen here (back to the agent);
// an agent's conversation ROOT (NEW-2: a root with `agentId`) redirects to
// the agent's page; an app's editor ROOT (`appId`) to its app editor,
// keeping `tab` / `open`.
//
// EXP-1291: old PR body footers link `sessions/<rootId>?view=guide`: that
// URL opens the chat. S-3: `back` = the inter-session back stack
// (`lib/session-back.ts`). EXP-1347: an issue pill of one of this chat's
// issues opens it in place (`IssueRefOutlet`).
type SessionSearch = ChatSearch & { view?: `guide` }

/** How long a missing root waits for its row before "not found". */
const SYNC_GRACE_MS = 4_000

export const Route = createFileRoute(`/t/$teamSlug/sessions/$sessionId`)({
  head: () => ({ meta: [{ title: pageTitle(CHATS_LIST_COPY.title) }] }),
  validateSearch: (search: Record<string, unknown>): SessionSearch => ({
    ...validateChatSearch(search),
    ...(search.view === `guide` ? { view: `guide` as const } : {}),
  }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: SessionPage,
})

function Loading() {
  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* D8 first load: the detail page's placeholder (3 list rows). */}
      <div className="p-6">
        <SkeletonRows />
      </div>
    </div>
  )
}

function SessionPage() {
  const { teamSlug, sessionId } = Route.useParams()
  const search = Route.useSearch()
  const { row } = useSessionRow(sessionId)
  // A child (its row says so) goes to its root; anything else is a root,
  // or a row still syncing (the chat page waits a grace for it).
  if (row && row.rootId && row.rootId !== row.id) {
    return <ChildSession teamSlug={teamSlug} row={row} rootId={row.rootId} search={search} />
  }
  return <ChatPage teamSlug={teamSlug} sessionId={sessionId} search={search} />
}

function ChildSession({
  teamSlug,
  row,
  rootId,
  search,
}: {
  teamSlug: string
  row: SessionRow
  rootId: string
  search: SessionSearch
}) {
  const { row: root, isReady: rootReady } = useSessionRow(rootId)
  const { data: auth } = useSession()
  // The root decides: an agent's conversation tree has no chat page.
  if (!root && !rootReady) return <Loading />
  if (root?.agentId && search.view !== `guide`) {
    // #EXP-1732: a session under MY conversation with the agent opens in
    // place on the agent's screen (its Sessions tab); anyone else's keeps
    // its own screen.
    if (root.userId && root.userId === auth?.user?.id) {
      return (
        <Navigate
          to="/t/$teamSlug/agents/$agentId"
          params={{ teamSlug, agentId: root.agentId }}
          search={{
            tab: `sessions` as const,
            open: `session` as const,
            session: row.id,
            ...(search.card ? { card: search.card } : {}),
            ...(search.back ? { back: search.back } : {}),
          }}
          replace
        />
      )
    }
    return <SessionScreen key={row.id} teamSlug={teamSlug} row={row} back={search.back} card={search.card} />
  }
  return (
    <Navigate
      to="/t/$teamSlug/sessions/$sessionId"
      params={{ teamSlug, sessionId: rootId }}
      search={
        search.view === `guide`
          ? {}
          : {
              tab: `sessions` as const,
              open: `session` as const,
              session: row.id,
              // DEEP LINKS: a card in the child rides along to its chat.
              ...(search.card ? { card: search.card } : {}),
              ...(search.from ? { from: search.from } : {}),
              ...(search.back ? { back: search.back } : {}),
            }
      }
      replace
    />
  )
}

function ChatPage({
  teamSlug,
  sessionId,
  search,
}: {
  teamSlug: string
  sessionId: string
  search: SessionSearch
}) {
  const team = useTeamBySlug(teamSlug)
  const navigate = useNavigate()
  const { apps, isReady: appsReady } = useApps(team?.id)
  const chat = useSessionChat(
    sessionId,
    search.open === `session` ? (search.session ?? null) : null
  )
  usePageTitle(
    chat.view && chat.titleReady
      ? pageTitle(chat.view.title, CHATS_LIST_COPY.title)
      : undefined
  )

  // A chat opened right after its start may land before its row: wait a
  // beat before saying it is gone.
  const [graceOver, setGraceOver] = useState(false)
  useEffect(() => {
    setGraceOver(false)
    const timer = setTimeout(() => setGraceOver(true), SYNC_GRACE_MS)
    return () => clearTimeout(timer)
  }, [sessionId])

  // EXP-1291 / S-3: an old PR footer's `?view=guide` opens the conversation.
  if (search.view === `guide`) {
    return (
      <Navigate
        to="/t/$teamSlug/sessions/$sessionId"
        params={{ teamSlug, sessionId }}
        search={{}}
        replace
      />
    )
  }

  // #EXP-1732: an app's editor session (a root with `appId`) is no chat
  // page: its page is the app editor (keeping `tab` / `open`).
  if (chat.root?.appId) {
    const app = apps.find((row) => row.id === chat.root!.appId)
    if (app) {
      return (
        <Navigate
          to="/t/$teamSlug/app-editor/$appSlug"
          params={{ teamSlug, appSlug: app.slug }}
          search={{
            ...(search.tab ? { tab: search.tab } : {}),
            ...(search.open ? { open: search.open } : {}),
            ...(search.open === `session` && search.session ? { session: search.session } : {}),
          }}
          replace
        />
      )
    }
    if (!appsReady) return <Loading />
  }

  // An agent's conversation (NEW-2: a root with `agentId`) is no chat page:
  // its page is the agent's.
  if (chat.root?.agentId) {
    return (
      <Navigate
        to="/t/$teamSlug/agents/$agentId"
        params={{ teamSlug, agentId: chat.root.agentId }}
        search={search.back ? { back: search.back } : {}}
        replace
      />
    )
  }

  // EXP-1316: until the issue shapes reported ready the title could still
  // flip from the request text to `Implement EXP-12` (and the chips pop in):
  // hold the page on its placeholder — but only for the sync grace: a shape
  // that never reports ready must not leave a permanent skeleton, so past it
  // the page renders with whatever it has (the title settles in place).
  const titleHeld = !chat.titleReady && !graceOver
  if (!team || !chat.view || !chat.root || titleHeld) {
    if (team && chat.isReady && !chat.root && graceOver) {
      return (
        <div className="flex h-full min-h-0 flex-col items-center justify-center">
          <Empty id="chat-not-found">
            <Button asChild variant="outline" size="sm">
              <Link to="/t/$teamSlug/agent" params={{ teamSlug }}>
                {emptyStateCopy(`chat-not-found`).action}
              </Link>
            </Button>
          </Empty>
        </div>
      )
    }
    return <Loading />
  }
  const { view: _view, ...chatSearch } = search
  return (
    <ChatScreen
      key={chat.root.id}
      team={team}
      teamSlug={teamSlug}
      view={chat.view}
      root={chat.root}
      rows={chat.rows}
      titles={chat.titles}
      pullRequests={chat.pullRequests}
      messagePage={chat.messagePage}
      search={chatSearch}
      onNavigate={(next, options) =>
        void navigate({
          to: `/t/$teamSlug/sessions/$sessionId`,
          params: { teamSlug, sessionId: chat.root!.id },
          search: next,
          replace: options?.replace,
        })
      }
    />
  )
}
