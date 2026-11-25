import { useEffect } from "react"
import { createFileRoute, Navigate, useNavigate, useRouter } from "@tanstack/react-router"
import { CHATS_COPY } from "@exp/domain-contract/chats"
import { conceptIcon, HeaderAction, MobileDetailHeader, useIsMobile } from "@exp/ui"
import { InboxView } from "@/components/inbox/inbox-view"
import { setInboxPopoverOpen } from "@/hooks/use-inbox-popover"
import { markAllNotificationsRead, useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import { pageTitle } from "@/lib/page-title"

const MarkReadIcon = conceptIcon(`notification-mark-read`)

// #FEED-191, #EXP-2109: `/inbox` is the PHONES' pushed page (chats.json): every
// team's notifications, its own back header with Mark all read. Wide the Inbox
// is a popover off the sidebar header, so this route opens it over Home. An old
// `?tab=drafts` link is the Drafts page, `?tab=my-issues` the list itself.
export const Route = createFileRoute(`/_app/inbox`)({
  head: () => ({ meta: [{ title: pageTitle(`Inbox`) }] }),
  validateSearch: (search: Record<string, unknown>): { tab?: `drafts` } =>
    search.tab === `drafts` ? { tab: `drafts` } : {},
  component: InboxRoute,
})

function InboxRoute() {
  const { tab } = Route.useSearch()
  const isMobile = useIsMobile()
  if (tab === `drafts`) return <Navigate to="/drafts" replace />
  return isMobile ? <PhoneInbox /> : <WideInbox />
}

function PhoneInbox() {
  const navigate = useNavigate()
  const router = useRouter()
  const unread = useUnreadNotificationCount()
  return (
    <div className="flex h-full flex-col pt-[env(safe-area-inset-top)]">
      <MobileDetailHeader
        title={CHATS_COPY.inbox}
        onBack={() => (router.history.canGoBack() ? router.history.back() : void navigate({ to: `/chats` }))}
        menu={
          unread > 0 ? (
            <HeaderAction aria-label="Mark all read" title="Mark all read" onClick={() => markAllNotificationsRead()}>
              <MarkReadIcon />
            </HeaderAction>
          ) : undefined
        }
      />
      <div className="min-h-0 flex-1">
        <InboxView from="inbox" />
      </div>
    </div>
  )
}

function WideInbox() {
  const navigate = useNavigate()
  useEffect(() => {
    setInboxPopoverOpen(true)
    void navigate({ to: `/home`, replace: true })
  }, [navigate])
  return null
}
