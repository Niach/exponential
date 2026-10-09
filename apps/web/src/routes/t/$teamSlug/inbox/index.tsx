import { useEffect, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { DraftsList } from "@/components/drafts-list"
import { InboxView } from "@/components/inbox/inbox-view"
import { MyIssuesView } from "@/components/my-issues-view"
import { ListDetailEmpty } from "@/components/team/list-detail"
import {
  TAB_BODY_TOUCH_CLASS,
  primeTabEnter,
  useFaceSwipe,
} from "@/components/work-faces"
import {
  Button,
  SEGMENTED_ROW,
  SEGMENTED_TAB,
  Tabs,
  TabsList,
  TabsTrigger,
  conceptIcon,
  useIsMobile,
} from "@exp/ui"
import { useDraftEntriesWithReady } from "@/hooks/use-issue-drafts"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import { trpc } from "@/lib/trpc-client"
import { pageTitle } from "@/lib/page-title"

// EXP-525: the tab segments carry the same registry glyphs the mobile My Work
// segments and the desktop rail use.
const InboxTabIcon = conceptIcon(`nav-inbox`)
const MyIssuesTabIcon = conceptIcon(`ui-assignee`)
// EXP-878: below md there is no Drafts route — the drafts list is a third
// segment here, present only while the caller actually has one.
const DraftsTabIcon = conceptIcon(`nav-drafts`)
const MarkReadIcon = conceptIcon(`notification-mark-read`)

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
// beside it. Phones keep the full-width list and push the detail.
type InboxSearch = {
  tab?: `my-issues` | `drafts`
}

type InboxTab = `inbox` | `my-issues` | `drafts`

export const Route = createFileRoute(`/t/$teamSlug/inbox/`)({
  head: ({ match }) => ({
    meta: [
      {
        title: pageTitle(
          match.search.tab === `my-issues`
            ? `My Issues`
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

// EXP-616: inside the capsule segment the count is a plain trailing number
// (the GlassSectionHeader idiom), not a second pill nested in a pill.
function UnreadTabCount() {
  const unread = useUnreadNotificationCount()
  if (unread === 0) return null
  return (
    <span className="text-xs text-foreground/50 tabular-nums">
      {unread > 99 ? `99+` : unread}
    </span>
  )
}

function MarkAllReadButton() {
  const unread = useUnreadNotificationCount()
  if (unread === 0) return null
  return (
    // EXP-878: on a phone the strip can hold three segments (Inbox, My
    // Issues, Drafts), which leaves no room for the label — the button
    // collapses to its glyph there and keeps the text for readers.
    <Button
      variant="ghost"
      size="sm"
      className="shrink-0"
      aria-label="Mark all read"
      onClick={() => void trpc.notifications.markAllRead.mutate()}
    >
      <MarkReadIcon className="md:hidden" />
      <span className="max-md:sr-only">Mark all read</span>
    </Button>
  )
}

function InboxPage() {
  const { teamSlug } = Route.useParams()
  const search = Route.useSearch()
  const navigate = useNavigate()
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const isMobile = useIsMobile()
  const { entries: draftEntries, isReady: draftsReady } =
    useDraftEntriesWithReady(team?.id)
  const draftCount = draftEntries.length
  // The segment only exists on a phone that has drafts; anywhere else the
  // sidebar owns the surface. Both conditions can stop holding while the tab
  // is open (the last draft is filed, the viewport widens), so the tab falls
  // back to the inbox rather than rendering a segment that is not there.
  // Until the shapes have synced, a requested `?tab=drafts` is taken at its
  // word: a cold deep link must not bounce before the rows arrive.
  const draftsTabAvailable =
    isMobile &&
    (draftCount > 0 || (!draftsReady && search.tab === `drafts`))
  const requestedTab: InboxTab =
    search.tab === `my-issues` || search.tab === `drafts`
      ? search.tab
      : `inbox`
  const tab: InboxTab =
    requestedTab === `drafts` && !draftsTabAvailable ? `inbox` : requestedTab
  const [bulkSlot, setBulkSlot] = useState<HTMLDivElement | null>(null)

  // EXP-1190: on a phone the body pages between the segments, like the Work
  // screen's faces.
  const tabs: InboxTab[] = draftsTabAvailable
    ? [`inbox`, `my-issues`, `drafts`]
    : [`inbox`, `my-issues`]
  const setTab = (next: InboxTab) => {
    void navigate({
      to: `/t/$teamSlug/inbox`,
      params: { teamSlug },
      search: {
        ...search,
        tab: next === `inbox` ? undefined : next,
      },
      replace: true,
    })
  }
  const swipe = useFaceSwipe(tabs, tab, setTab)

  useEffect(() => {
    if (!draftsReady) return
    if (requestedTab === `drafts` && !draftsTabAvailable) {
      void navigate({
        to: `/t/$teamSlug/inbox`,
        params: { teamSlug },
        search: { tab: undefined },
        replace: true,
      })
    }
  }, [requestedTab, draftsTabAvailable, draftsReady, navigate, teamSlug])

  if (!session?.user) return null

  if (!isMobile) {
    return (
      <ListDetailEmpty tab={tab === `my-issues` ? `my-issues` : `inbox`}>
        {tab !== `my-issues` && <MarkAllReadButton />}
      </ListDetailEmpty>
    )
  }

  return (
    <div className="flex h-full flex-col" {...(isMobile ? swipe : {})}>
      {/* EXP-851: `SEGMENTED_ROW` — the same padding the Support strip sits
          in, so the two read as one control at one size. */}
      <div className={SEGMENTED_ROW}>
        <div className="flex min-w-0 items-center gap-2">
          {/* EXP-616: the capsule segmented control, still URL-driven — the
              controlled value is the parsed ?tab and every change navigates. */}
          <Tabs
            value={tab}
            onValueChange={(next) => {
              primeTabEnter(tabs, tab, next as InboxTab)
              setTab(next as InboxTab)
            }}
            className="w-fit shrink-0"
          >
            <TabsList>
              <TabsTrigger value="inbox" className={SEGMENTED_TAB}>
                <InboxTabIcon />
                Inbox
                <UnreadTabCount />
              </TabsTrigger>
              <TabsTrigger value="my-issues" className={SEGMENTED_TAB}>
                <MyIssuesTabIcon />
                My Issues
              </TabsTrigger>
              {draftsTabAvailable && (
                <TabsTrigger value="drafts" className={SEGMENTED_TAB}>
                  <DraftsTabIcon />
                  Drafts
                  {draftCount > 0 && (
                    <span className="text-xs text-foreground/50 tabular-nums">
                      {draftCount}
                    </span>
                  )}
                </TabsTrigger>
              )}
            </TabsList>
          </Tabs>
          {/* The My Issues bulk-action bar portals in here (EXP-525) so a
              selection never reflows the list under it; EXP-642 moved the
              slot LEFT, beside the tabs. */}
          {tab === `my-issues` && (
            <div ref={setBulkSlot} className="contents" />
          )}
        </div>
        {tab === `inbox` ? <MarkAllReadButton /> : null}
      </div>

      <div
        className={isMobile ? `min-h-0 flex-1 ${TAB_BODY_TOUCH_CLASS}` : `min-h-0 flex-1`}
        data-face-body={isMobile ? `` : undefined}
      >
        {tab === `inbox` ? (
          <InboxView from="inbox" />
        ) : tab === `drafts` ? (
          <DraftsList
            teamId={team?.id}
            teamSlug={teamSlug}
            from="inbox:drafts"
            className="px-4 py-3"
          />
        ) : (
          <MyIssuesView
            teamSlug={teamSlug}
            bulkActionSlot={bulkSlot}
          />
        )}
      </div>
    </div>
  )
}
