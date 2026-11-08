import { useEffect, useState } from "react"
import { createFileRoute, Link, Navigate, redirect, useNavigate } from "@tanstack/react-router"
import { Button, SkeletonRows } from "@exp/ui"
import { Empty } from "@/components/empty"
import { ChatScreen, validateChatSearch, type ChatSearch } from "@/components/chat/chat-screen"
import { SessionScreen } from "@/components/chat/session-body"
import { emptyStateCopy } from "@/lib/empty-states"
import { useSessionChat, useSessionRow } from "@/hooks/use-session-tree"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import { CHATS_LIST_COPY, type SessionRow } from "@/lib/session-tree"

// EXP-1733: ONE route per session. A chat IS a root session, so
// `/sessions/$id` of a ROOT renders THE chat screen (fixture
// `session-chat.json` ×4, `buildSessionChat`). Header: the title (EXP-1316:
// derived, `Implement EXP-12` / `Run <action>` / the agent's auto-name;
// nobody renames it; a lock when private) + ONE tab strip that never
// scrolls (#EXP-1433, `ChatTabs`): "+" (THE chat menu) · Conversation | the
// one issue chip OR ONE group tab (every issue and session in THE Menu) |
// Results. md+: while the Guide's PR is open the header's trailing slot
// carries Merge on EVERY tab, then the `…` (Guide tab only).
// `?tab=` names the tab on show:
//
//   (absent)               the conversation = the DISPATCHER (EXP-1334: no
//                          card of its own): its messages, THE session card
//                          per child, question and plan cards (its own and
//                          its children's) answered in the card, EXP-1323:
//                          the suggested tasks card, report lines and the
//                          live tail (Tasks card + status line), the
//                          composer at the bottom
//   issue&issue=<id>       that issue's body, embedded (no header bar, no
//                          Start, no pin, no Chats row)
//   results[&section=&file=] the root's Results over its pull request
//   session&session=<id>   EXP-1316: that session's RAW WORK in place under
//                          this header + strip, its Stop pill in the header
//                          (EXP-1343: phones too, no own screen), picked
//                          from the group tab's tree rows.
//
// A CHILD's `/sessions/$id` REDIRECTS (replace) to its root with
// `?tab=session&session=<id>`, keeping `from` and `back` (deep links,
// notifications, the work-tab strip). A session under a team AGENT has no
// chat page: it keeps its own screen here (back to the agent's channel); an
// agent's conversation ROOT (NEW-2: a root with `agentId`) redirects to the
// agent's page.
//
// EXP-1291: old PR body footers link `sessions/<rootId>?view=guide`; S-3:
// the Guide tab is gone, so that URL opens the chat's conversation.
// S-3: `back` = the inter-session back stack (`lib/session-back.ts`).
//
// Phones: the strip IS the pager under the header (#EXP-1433: Conversation ·
// the one issue | the item open from the group · Results — swipe or tap,
// `lib/chat-pages.ts`). No right sidebar, no Show work, no captions.
// EXP-1347: an issue pill of one of this chat's issues selects its tab
// (`IssueRefOutlet`).
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
  // The root decides: an agent's conversation tree has no chat page.
  if (!root && !rootReady) return <Loading />
  if (root?.agentId && search.view !== `guide`) {
    return <SessionScreen key={row.id} teamSlug={teamSlug} row={row} back={search.back} />
  }
  return (
    <Navigate
      to="/t/$teamSlug/sessions/$sessionId"
      params={{ teamSlug, sessionId: rootId }}
      search={
        search.view === `guide`
          ? {}
          : {
              tab: `session` as const,
              session: row.id,
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
  const chat = useSessionChat(
    sessionId,
    search.tab === `session` ? (search.session ?? null) : null
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
