import { createFileRoute, Navigate, redirect } from "@tanstack/react-router"
import { useIsMobile } from "@exp/ui"
import { IssueDraftPage } from "@/components/issue-draft-page"
import { resolveBoardTarget } from "@/components/team/mobile-tab-bar"
import {
  useTeamBoardsWithReady,
  useTeamBySlug,
  useTeamLabelsWithReady,
  useTeamUsers,
} from "@/hooks/use-team-data"
import { useIssueDraft, useTeamDraftsWithReady } from "@/hooks/use-issue-drafts"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { useSession } from "@/hooks/use-session"
import {
  ISSUE_DRAFT_COPY,
  parseIssueDraftSearch,
} from "@/lib/issue-draft-page"
import { pageTitle } from "@/lib/page-title"

// EXP-1170: the New issue PAGE. Every "New issue" opener mints a draft id at
// tap time and lands here (`newDraftNavigation`); a Drafts row reopens its
// own id. `?board=` = the board it files onto (absent: the team's default),
// `?status=` = the group status a "+" seeded, `?from=` = the origin list
// (md+ keeps it beside the page, Back returns to it).
export const Route = createFileRoute(`/t/$teamSlug/drafts/$draftId`)({
  head: () => ({ meta: [{ title: pageTitle(ISSUE_DRAFT_COPY.header) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  validateSearch: parseIssueDraftSearch,
  component: IssueDraftRoute,
})

function IssueDraftRoute() {
  const { teamSlug, draftId } = Route.useParams()
  const search = Route.useSearch()
  const isMobile = useIsMobile()
  const team = useTeamBySlug(teamSlug)
  const { boards, boardsReady } = useTeamBoardsWithReady(team?.id)
  const { users, members } = useTeamUsers(team?.id)
  const { labels, labelsReady } = useTeamLabelsWithReady(team?.id)
  const { isReady: draftsReady } = useTeamDraftsWithReady(team?.id)
  const draft = useIssueDraft(draftId)
  const permissions = useTeamPermissions(team)
  // `canCreate` reads the session too: on a cold load it may still be
  // pending, and deciding then would bounce a member to the team home.
  const { data: session, isPending: sessionPending } = useSession()
  const sessionReady = !sessionPending && Boolean(session?.user)
  // `members` empty = still syncing (a member always sees themselves).
  const membersReady = members.length > 0 && users.length > 0

  // Seeding happens ONCE, on mount, so the page waits for everything a seed
  // resolves against.
  if (
    !team ||
    !boardsReady ||
    !draftsReady ||
    !labelsReady ||
    !membersReady ||
    !sessionReady
  ) {
    return (
      <div className="p-6 text-sm text-muted-foreground">Loading…</div>
    )
  }

  if (boards.length === 0 || !permissions.canCreate) {
    return <Navigate to="/t/$teamSlug" params={{ teamSlug }} replace />
  }

  // A reopened draft whose board is gone (trashed, archived) has nowhere to
  // file: back to the list it came from.
  if (draft && !boards.some((board) => board.id === draft.boardId)) {
    return isMobile ? (
      <Navigate
        to="/t/$teamSlug/inbox"
        params={{ teamSlug }}
        search={{ tab: `drafts` }}
        replace
      />
    ) : (
      <Navigate to="/t/$teamSlug/drafts" params={{ teamSlug }} replace />
    )
  }

  const initialBoard =
    boards.find((board) => board.id === search.board) ??
    resolveBoardTarget(teamSlug, boards, undefined) ??
    boards[0]

  return (
    <IssueDraftPage
      key={draftId}
      draftId={draftId}
      teamId={team.id}
      teamSlug={teamSlug}
      boards={boards}
      users={users}
      labels={labels}
      draft={draft}
      initialBoardId={initialBoard.id}
      initialStatusId={search.status}
      from={search.from}
    />
  )
}
