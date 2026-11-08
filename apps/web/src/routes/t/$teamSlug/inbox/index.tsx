import { useEffect } from "react"
import { createFileRoute, redirect, useNavigate, useRouter } from "@tanstack/react-router"
import { CHATS_COPY } from "@exp/domain-contract/chats"
import { InboxView } from "@/components/inbox/inbox-view"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import { resolveBoardTarget } from "@/components/team/mobile-tab-bar"
import { Pill, useIsMobile } from "@exp/ui"
import { useSession } from "@/hooks/use-session"
import { useTeamBoardsWithReady, useTeamBySlug } from "@/hooks/use-team-data"
import { markAllNotificationsRead, useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import { setInboxPopoverOpen } from "@/hooks/use-inbox-popover"
import { pageTitle } from "@/lib/page-title"

// #FEED-191: `/inbox` is the PHONES' pushed page (`PhoneInboxPage`, chats.json).
// Wide the Inbox is a popover off the sidebar header, so this route opens it and
// redirects home. On both, an old `?tab=my-issues` / `?tab=drafts` link forwards
// to the board's My issues (phones: its Drafts chip; wide: the Drafts page).
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

/** An old `?tab=` link: the board's chip it moved to. */
function useForwardInboxTab(teamId: string | undefined) {
  const { teamSlug } = Route.useParams()
  const { tab } = Route.useSearch()
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const { boards, boardsReady } = useTeamBoardsWithReady(teamId)
  useEffect(() => {
    if (!tab || !boardsReady) return
    if (tab === `drafts` && !isMobile) {
      void navigate({ to: `/t/$teamSlug/drafts`, params: { teamSlug }, replace: true })
      return
    }
    const board = resolveBoardTarget(teamSlug, boards, undefined)
    if (!board) {
      void navigate({ to: `/t/$teamSlug/inbox`, params: { teamSlug }, search: {}, replace: true })
      return
    }
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug`,
      params: { teamSlug, boardSlug: board.slug },
      search: { chip: tab === `drafts` ? `drafts` : `mine` },
      replace: true,
    })
  }, [tab, boardsReady, boards, navigate, teamSlug, isMobile])
}

// Chat 58cb32df (chats.json `shell`): on phones the Inbox is the
// inbox ONLY — pushed from the Chats header's inbox circle, its own back
// header with Mark all read, no segments, the ONE active team's stream. My
// issues and Drafts moved to the board's chips, so an old `?tab=my-issues`
// / `?tab=drafts` link forwards to that chip.
function PhoneInboxPage() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const router = useRouter()
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const unread = useUnreadNotificationCount(team?.id)

  useForwardInboxTab(team?.id)

  if (!session?.user) return null

  const back = () => {
    if (router.history.canGoBack()) router.history.back()
    else void navigate({ to: `/t/$teamSlug/chats`, params: { teamSlug } })
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
              onClick={() => markAllNotificationsRead(team?.id)}
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

// md+: no page: the popover opens over home.
function WideInboxPage() {
  const { teamSlug } = Route.useParams()
  const { tab } = Route.useSearch()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  useForwardInboxTab(team?.id)
  useEffect(() => {
    if (tab) return
    setInboxPopoverOpen(true)
    void navigate({ to: `/t/$teamSlug/agent`, params: { teamSlug }, replace: true })
  }, [tab, navigate, teamSlug])
  return null
}
