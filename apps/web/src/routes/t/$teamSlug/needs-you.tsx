import { createFileRoute, redirect, useNavigate, useRouter } from "@tanstack/react-router"
import { NEEDS_YOU_COPY } from "@exp/domain-contract/needs-you"
import { CHATS_COPY } from "@exp/domain-contract/chats"
import { Button, conceptIcon, NeedsYouList, SkeletonRows, useIsMobile } from "@exp/ui"
import { MobilePageHeader } from "@/components/team/mobile-page-header"
import { NeedsYouStuckActions } from "@/components/needs-you-stuck-actions"
import { useNeedsYou, useOpenNeedsYou } from "@/hooks/use-needs-you"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { pageTitle } from "@/lib/page-title"

// #FEED-191 NEEDS YOU (fixtures/needs-you.json `route`): every plan and question
// waiting on the viewer. Wide: the content card, no header. Phones: pushed, the
// page header with the back control, like the Archived page. The route stays
// reachable with no row (the last answer): THE EmptyState.

const UiBackIcon = conceptIcon(`ui-back`)

export const Route = createFileRoute(`/t/$teamSlug/needs-you`)({
  head: () => ({ meta: [{ title: pageTitle(NEEDS_YOU_COPY.entry) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({ to: `/auth/login`, search: { redirect: location.href } })
    }
  },
  component: NeedsYouPage,
})

function NeedsYouPage() {
  const isMobile = useIsMobile()
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const router = useRouter()
  const team = useTeamBySlug(teamSlug)
  const { data: authSession } = useSession()
  const me = authSession?.user?.id ?? null
  const permissions = useTeamPermissions(team)
  const { rows, isReady } = useNeedsYou(team?.id, me, permissions.isOwner)
  const open = useOpenNeedsYou(teamSlug, me)
  const back = () => {
    if (router.history.canGoBack()) router.history.back()
    else void navigate({ to: `/t/$teamSlug/threads`, params: { teamSlug } })
  }
  return (
    <div className="h-full overflow-y-auto">
      <div
        className={`mx-auto flex w-full max-w-3xl flex-col gap-3 px-4 ${
          isMobile ? `pt-[max(1rem,env(safe-area-inset-top))] pb-6` : `py-4`
        }`}
      >
        {isMobile && (
          <MobilePageHeader
            teamSlug={teamSlug}
            title={NEEDS_YOU_COPY.entry}
            leading={
              <Button
                variant="ghost"
                size="icon"
                aria-label={CHATS_COPY.title}
                className="-ml-2 size-9 shrink-0 rounded-full text-muted-foreground hover:text-foreground"
                onClick={back}
              >
                <UiBackIcon className="size-5" />
              </Button>
            }
          />
        )}
        {!team || (!isReady && rows.length === 0) ? <SkeletonRows /> : (
          <NeedsYouList rows={rows} onOpen={open} actions={(row) => <NeedsYouStuckActions row={row} />} />
        )}
      </div>
    </div>
  )
}
