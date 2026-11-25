import { useEffect, useMemo, useRef, useState } from "react"
import { createFileRoute, Navigate, redirect, useNavigate } from "@tanstack/react-router"
import { issueDraftBoards } from "@exp/domain-contract/issue-draft"
import { SkeletonRows, useIsMobile } from "@exp/ui"
import { useMemberTeams } from "@/components/home/use-member-teams"
import { IssueDraftPage } from "@/components/issue-draft-page"
import { resolveBoardTarget } from "@/components/team/mobile-tab-bar"
import {
  useBoardsForTeams,
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
import { loadPendingShare, type PendingShare } from "@/lib/share/pending-share"

// EXP-1170: the New issue PAGE. Every "New issue" opener mints a draft id at
// tap time and lands here (`newDraftNavigation`); a Drafts row reopens its
// own id. `?board=` = the board it files onto (absent: the team's default),
// `?status=` = the group status a "+" seeded, `?from=` = the origin list
// (md+ keeps it beside the page, Back returns to it), EXP-1635 `?share=` = the
// pending share prefills it and Back returns to Share to.
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
  const { users, members } = useTeamUsers(team?.id)
  const { labels, labelsReady } = useTeamLabelsWithReady(team?.id)
  const { isReady: draftsReady } = useTeamDraftsWithReady(team?.id)
  const draft = useIssueDraft(draftId)
  const permissions = useTeamPermissions(team)
  // `canCreate` reads the session too: on a cold load it may still be
  // pending, and deciding then would bounce a member to the team home.
  const { data: session, isPending: sessionPending } = useSession()
  const sessionReady = !sessionPending && Boolean(session?.user)
  // #EXP-2125 (issue-draft.json `boards`): every member team's boards; a
  // chat's, a share's draft stays in its team.
  const { teams, teamIds, byId } = useMemberTeams(session?.user?.id)
  const { boards: allBoards, boardsReady } = useBoardsForTeams(teamIds)
  const only = search.chat || search.share || draft?.sessionId ? team?.id : null
  const offered = useMemo(() => {
    const { boards, showTeam } = issueDraftBoards({
      teams: teams.map((row) => ({ id: row.id, name: row.name, kind: `team` })),
      boards: allBoards,
      only,
    })
    return boards.map((board) => ({ ...board, teamName: showTeam ? (byId.get(board.teamId)?.name ?? null) : null }))
  }, [teams, byId, allBoards, only])
  const boards = useMemo(() => offered.filter((board) => board.teamId === team?.id), [offered, team?.id])
  const navigate = useNavigate()
  // A pick of another team's board moves the open page's URL: what it holds
  // stays mounted while the new team's rows load.
  const openedRef = useRef<string | null>(null)
  const opened = openedRef.current === draftId
  // `members` empty = still syncing (a member always sees themselves).
  const membersReady = members.length > 0 && users.length > 0

  // undefined = still loading; the page seeds from it once.
  const [share, setShare] = useState<PendingShare | null | undefined>(
    search.share ? undefined : null
  )
  useEffect(() => {
    if (!search.share) return
    let current = true
    void loadPendingShare().then((pending) => {
      if (current) setShare(pending)
    })
    return () => {
      current = false
    }
  }, [search.share])

  // Seeding happens ONCE, on mount, so the page waits for everything a seed
  // resolves against.
  if (
    share === undefined ||
    !team ||
    (!opened && (!boardsReady || !draftsReady || !labelsReady || !membersReady || !sessionReady))
  ) {
    // D8 first load: the detail page's placeholder (3 list rows).
    return (
      <div className="p-6">
        <SkeletonRows />
      </div>
    )
  }

  if (!opened && (boards.length === 0 || !permissions.canCreate)) {
    return <Navigate to="/t/$teamSlug" params={{ teamSlug }} replace />
  }

  // A reopened draft whose board is gone (trashed, archived) has nowhere to
  // file: back to the list it came from.
  const draftBoard = draft && offered.find((board) => board.id === draft.boardId)
  const draftTeam = draftBoard && draftBoard.teamId !== team.id ? byId.get(draftBoard.teamId) : undefined
  // #EXP-2125: moved to another team meanwhile: its page is that team's.
  if (!opened && draftTeam) {
    return (
      <Navigate to="/t/$teamSlug/drafts/$draftId" params={{ teamSlug: draftTeam.slug, draftId }} search={search} replace />
    )
  }
  if (!opened && draft && !draftBoard) {
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

  openedRef.current = draftId
  const initialBoard =
    boards.find((board) => board.id === search.board) ??
    resolveBoardTarget(teamSlug, boards, undefined) ??
    boards[0] ??
    offered[0]
  if (!initialBoard) return null

  return (
    <IssueDraftPage
      key={draftId}
      draftId={draftId}
      teamId={team.id}
      teamSlug={teamSlug}
      boards={offered}
      onTeamChange={(board) => {
        const next = byId.get(board.teamId)
        if (!next) return
        void navigate({
          to: `/t/$teamSlug/drafts/$draftId`,
          params: { teamSlug: next.slug, draftId },
          search: { ...search, board: board.id, status: undefined },
          replace: true,
          ignoreBlocker: true,
        })
      }}
      users={users}
      labels={labels}
      draft={draft}
      initialBoardId={initialBoard.id}
      initialStatusId={search.status}
      from={search.from}
      chatId={search.chat}
      fromShare={Boolean(search.share)}
      share={share}
    />
  )
}
