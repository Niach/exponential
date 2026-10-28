import { useEffect, useState } from "react"
import { createFileRoute, Navigate, redirect, useNavigate } from "@tanstack/react-router"
import { conceptIcon, EmptyState, SkeletonRows } from "@exp/ui"
import { ThreadScreen, validateThreadSearch } from "@/components/thread/thread-screen"
import { useSessionThread } from "@/hooks/use-session-tree"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import { THREADS_LIST_COPY } from "@/lib/session-tree"

// EXP-1293 Threads v2: a THREAD = a root session (fixture
// `session-thread.json` ×4, `buildSessionThread`). Header: the title
// (EXP-1316: derived, `Implement EXP-12` / `Run <action>` / the agent's
// auto-name; nobody renames it; a lock when private) + ONE tab strip that
// never scrolls (EXP-1334, `ThreadTabs`): "+" (THE thread menu) ·
// Conversation | issue chips … +X | session chips … +Y | Guide (a dot while
// it holds anything). md+: while the Guide's PR is
// open the header's trailing slot carries Merge on EVERY tab, then the `…`
// (Guide tab only).
// `?tab=` names the tab on show:
//
//   (absent)               the conversation = the DISPATCHER (EXP-1334: no
//                          card of its own): its messages, THE session card
//                          per child, question and plan cards (its own and
//                          its children's)
//                          answered in the card, EXP-1323: the suggested
//                          tasks card, report lines and the live tail (Tasks
//                          card + status line), the composer at the bottom
//   issue&issue=<id>       that issue's body, embedded (no header bar, no
//                          Start, no pin, no Threads row)
//   guide[&section=&file=] the root's Guide over its pull request (its stack
//                          rail; phones: THE bottom Merge capsule)
//   session&session=<id>   EXP-1316: that session's RAW WORK in place under
//                          this header + strip, its Stop pill in the header
//                          (EXP-1343: phones too, no own screen). The
//                          session chips = the dispatcher's direct children;
//                          a chip with descendants opens its subtree on a
//                          CLICK. `/sessions/$id` redirects here.
//
// Phones: the strip IS the pager under the header (EXP-1343: every strip
// tab is a page — swipe or tap, in strip order, `lib/thread-pages.ts`). No
// right sidebar, no Show work, no captions. EXP-1347: an issue pill of one
// of this thread's issues selects its tab (`IssueRefOutlet`).

/** How long a missing root waits for its row before "not found". */
const SYNC_GRACE_MS = 4_000

const ThreadsIcon = conceptIcon(`nav-threads`)

export const Route = createFileRoute(`/t/$teamSlug/threads/$threadId`)({
  head: () => ({ meta: [{ title: pageTitle(THREADS_LIST_COPY.title) }] }),
  validateSearch: validateThreadSearch,
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: ThreadPage,
})

function ThreadPage() {
  const { teamSlug, threadId } = Route.useParams()
  const team = useTeamBySlug(teamSlug)
  const search = Route.useSearch()
  const navigate = useNavigate()
  const thread = useSessionThread(
    threadId,
    search.tab === `session` ? (search.session ?? null) : null
  )
  usePageTitle(
    thread.view && thread.titleReady
      ? pageTitle(thread.view.title, THREADS_LIST_COPY.title)
      : undefined
  )

  // A thread opened right after its start may land before its row: wait a
  // beat before saying it is gone.
  const [graceOver, setGraceOver] = useState(false)
  useEffect(() => {
    setGraceOver(false)
    const timer = setTimeout(() => setGraceOver(true), SYNC_GRACE_MS)
    return () => clearTimeout(timer)
  }, [threadId])

  // An agent's conversation (NEW-2: a root with `agentId`) is no thread:
  // its page is the agent's.
  if (thread.root?.agentId) {
    return (
      <Navigate
        to="/t/$teamSlug/agents/$agentId"
        params={{ teamSlug, agentId: thread.root.agentId }}
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
  const titleHeld = !thread.titleReady && !graceOver
  if (!team || !thread.view || !thread.root || titleHeld) {
    if (team && thread.isReady && !thread.root && graceOver) {
      return (
        <div className="flex h-full flex-col items-center justify-center">
          <EmptyState
            icon={ThreadsIcon}
            title="Not found"
            description="It may still be syncing, or it was deleted."
            size="page"
          />
        </div>
      )
    }
    return (
      <div className="p-6">
        <SkeletonRows />
      </div>
    )
  }
  return (
    <ThreadScreen
      key={thread.root.id}
      team={team}
      teamSlug={teamSlug}
      view={thread.view}
      root={thread.root}
      rows={thread.rows}
      titles={thread.titles}
      pullRequests={thread.pullRequests}
      messagePage={thread.messagePage}
      search={search}
      onNavigate={(next, options) =>
        void navigate({
          to: `/t/$teamSlug/threads/$threadId`,
          params: { teamSlug, threadId: thread.root!.id },
          search: next,
          replace: options?.replace,
        })
      }
    />
  )
}

