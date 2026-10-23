import { useEffect } from "react"
import { createFileRoute, redirect, useNavigate, useRouter } from "@tanstack/react-router"
import { CHATS_COPY } from "@exp/domain-contract/chats"
import { InboxView } from "@/components/inbox/inbox-view"
import { ListDetailEmpty } from "@/components/team/list-detail"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import { resolveBoardTarget } from "@/components/team/mobile-tab-bar"
import { Pill, useIsMobile } from "@exp/ui"
import { useSession } from "@/hooks/use-session"
import { useTeamBoardsWithReady, useTeamBySlug } from "@/hooks/use-team-data"
import { useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import { trpc } from "@/lib/trpc-client"
import { pageTitle } from "@/lib/page-title"

// The merged personal surface (EXP-186): ONE sidebar entry ("Inbox") with two
// tabs — the notification stream and the cross-board My Issues list — matching
// the mobile apps' segmented My Work screen. The active tab lives in the URL
// (?tab=my-issues; absent = inbox) so both tabs stay shareable and survive
// refresh.
//
// EXP-1246: md+ the Inbox is the LIST-DETAIL host's page — the list sits in
// the sidebar's panel slot (`components/team/list-detail.tsx`, from the URL:
// `sidebarOccupant`) and this page is the detail side with nothing picked
// (desktop's "No notification selected"). A row opens the issue's own route
// with `?from=inbox` / `?from=inbox:my-issues`, so the very same list stays
// beside it. Phones: the inbox only (`PhoneInboxPage`, chats.json).
type InboxSearch = {
  tab?: `my-issues` | `drafts`
}

export const Route = createFileRoute(`/t/$teamSlug/inbox/`)({
  head: ({ match }) => ({
    meta: [
      {
        title: pageTitle(
          match.search.tab === `my-issues`
            ? `My issues`
            : match.search.tab === `drafts`
              ? `Drafts`
              : `Inbox`
        ),
      },
    ],
  }),
  validateSearch: (search: Record<string, unknown>): InboxSearch => ({
    tab:
      search.tab === `my-issues` || search.tab === `drafts`
        ? search.tab
        : undefined,
  }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: InboxPage,
})

function InboxPage() {
  const isMobile = useIsMobile()
  return isMobile ? <PhoneInboxPage /> : <WideInboxPage />
}

// Thread 58cb32df (chats.json `shell`): on phones the Inbox is the
// inbox ONLY — pushed from the Chats header's inbox circle, its own back
// header with Mark all read, no segments, the ONE active team's stream. My
// issues and Drafts moved to the board's chips, so an old `?tab=my-issues`
// / `?tab=drafts` link forwards to that chip.
function PhoneInboxPage() {
  const { teamSlug } = Route.useParams()
  const search = Route.useSearch()
  const navigate = useNavigate()
  const router = useRouter()
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const { boards, boardsReady } = useTeamBoardsWithReady(team?.id)
  const unread = useUnreadNotificationCount()

  useEffect(() => {
    if (!search.tab || !boardsReady) return
    const board = resolveBoardTarget(teamSlug, boards, undefined)
    if (!board) {
      void navigate({ to: `/t/$teamSlug/inbox`, params: { teamSlug }, search: {}, replace: true })
      return
    }
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug`,
      params: { teamSlug, boardSlug: board.slug },
      search: { chip: search.tab === `drafts` ? `drafts` : `mine` },
      replace: true,
    })
  }, [search.tab, boardsReady, boards, navigate, teamSlug])

  if (!session?.user) return null

  const back = () => {
    if (router.history.canGoBack()) router.history.back()
    else void navigate({ to: `/t/$teamSlug/threads`, params: { teamSlug } })
  }

  return (
    <div className="flex h-full flex-col pt-[env(safe-area-inset-top)]">
      <MobileDetailHeader
        title={CHATS_COPY.inbox}
        onBack={back}
        menu={
          unread > 0 ? (
            <Pill
              mode="action"
              size="md"
              className="shrink-0"
              onClick={() => void trpc.notifications.markAllRead.mutate()}
            >
              Mark all read
            </Pill>
          ) : undefined
        }
      />
      <div className="min-h-0 flex-1">
        <InboxView from="inbox" teamId={team?.id} />
      </div>
    </div>
  )
}

// md+: the list-detail host's page with nothing picked (the list sits in
// the sidebar's panel slot, EXP-1246).
function WideInboxPage() {
  const search = Route.useSearch()
  const { data: session } = useSession()
  if (!session?.user) return null
  return <ListDetailEmpty tab={search.tab === `my-issues` ? `my-issues` : `inbox`} />
}
