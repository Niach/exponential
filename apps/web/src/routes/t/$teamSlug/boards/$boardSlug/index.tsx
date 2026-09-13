import { useEffect, useMemo, useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { BoardNotFound } from "@/components/board-not-found"
import { BulkActionBar } from "@/components/bulk-action-bar"
import { CreateIssueDialog } from "@/components/create-issue-dialog"
import { GettingStartedSection } from "@/components/getting-started/getting-started-section"
import { IssueList } from "@/components/issue-list"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { Button } from "@/components/ui/button"
import { useBoardViewData } from "@/hooks/use-board-view-data"
import { useIssueSearch } from "@/hooks/use-issue-search"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { conceptIcon } from "@/lib/icons.generated"
import type { StatusRowOption } from "@/lib/team-statuses"

// EXP-317: the cross-client nav glyphs come from the shared registry.
const NavSearchIcon = conceptIcon(`nav-search`)

// validateSearch drops anything unrecognised.
type BoardSearch = {
  description?: string
  /** EXP-856: the origin token a detail hands BACK when it returns here
   *  (`lib/detail-origin.ts`). A board is a list screen, so the sidebar keeps
   *  its main menu either way — the param only has to survive the round trip
   *  instead of being dropped on the way in. */
  from?: string
  new?: 1
  title?: string
}

export const Route = createFileRoute(
  `/t/$teamSlug/boards/$boardSlug/`
)({
  validateSearch: (search: Record<string, unknown>): BoardSearch => ({
    new: search.new === 1 || search.new === `1` ? 1 : undefined,
    title: typeof search.title === `string` ? search.title : undefined,
    description:
      typeof search.description === `string` ? search.description : undefined,
    from:
      typeof search.from === `string` && search.from ? search.from : undefined,
  }),
  component: BoardPage,
})

function BoardPage() {
  const { boardSlug, teamSlug } = Route.useParams()
  const search = Route.useSearch()
  const navigate = useNavigate()
  const issueSearch = useIssueSearch()
  const [createIssueOpen, setCreateIssueOpen] = useState(false)
  const [defaultStatus, setDefaultStatus] = useState<
    StatusRowOption | undefined
  >()
  const [prefill, setPrefill] = useState<
    { title?: string; description?: string } | undefined
  >(undefined)

  useEffect(() => {
    if (search.new === 1 || search.title || search.description) {
      setCreateIssueOpen(true)
      setPrefill({
        title: search.title,
        description: search.description,
      })
      // Clear only the one-shot create keys.
      void navigate({
        to: `/t/$teamSlug/boards/$boardSlug`,
        params: { teamSlug, boardSlug },
        search: (prev) => ({
          ...prev,
          new: undefined,
          title: undefined,
          description: undefined,
        }),
        replace: true,
      })
    }
  }, [search.new, search.title, search.description, navigate, teamSlug, boardSlug])

  const {
    issueLabelMap,
    issuesReady,
    labelList,
    board,
    boardReady,
    users,
    userMap,
    visibleGroups,
    team,
  } = useBoardViewData({
    boardSlug,
    teamSlug,
  })

  const permissions = useTeamPermissions(team)

  // Bulk-selection state lives here so the action bar can render in the
  // header region above the list (EXP-251);
  // IssueList keeps all the selection mechanics.
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const selectedIssues = useMemo(
    () =>
      visibleGroups
        .flatMap((group) => group.issues)
        .filter((issue) => selectedIds.has(issue.id)),
    [visibleGroups, selectedIds]
  )

  const handleNewIssue = (status?: StatusRowOption) => {
    if (!permissions.canCreate) return
    setDefaultStatus(status)
    setCreateIssueOpen(true)
  }

  if (!board || !team) {
    // `boardReady` implies the team resolved and the boards snapshot landed,
    // so an absent board here is definitive, not a sync gap (REV2-59).
    if (boardReady) {
      return (
        <BoardNotFound
          boardSlug={boardSlug}
          teamSlug={teamSlug}
        />
      )
    }
    return (
      <div className="text-muted-foreground text-sm p-6">
        Loading board...
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      {/* EXP-449: title-less control row — the page name lives in the
          sidebar/topbar and the New-issue button moved into the sidebar
          header, so this bar is just the left-hand actions plus the mobile
          Search button. Fixed height so hosting the bulk action bar here
          never reflows the list below (FEED-12); below md the bar floats
          itself above the tab bar. */}
      <div className="px-4 md:px-6">
        <div className="flex h-14 items-center justify-between gap-2">
          {/* EXP-642: the bulk bar sits LEFT. */}
          <div className="flex min-w-0 items-center gap-1">
            {selectedIssues.length > 0 ? (
              <BulkActionBar
                issues={selectedIssues}
                issueLabelMap={issueLabelMap}
                labels={labelList}
                users={users}
                teamId={team.id}
                onClear={() => setSelectedIds(new Set())}
              />
            ) : null}
          </div>
          <div className="flex shrink-0 items-center gap-1">
            {/* EXP-686: Search left the mobile tab bar and sits in the board
                header — native parity. The desktop sidebar header already
                carries its own Search button. */}
            <Button
              variant="ghost"
              size="icon"
              className="size-8 text-muted-foreground hover:text-foreground md:hidden"
              aria-label="Search"
              onClick={issueSearch.open}
            >
              <NavSearchIcon className="size-4" />
            </Button>
          </div>
        </div>
      </div>

      <div
        // EXP-698 r5: one clearance for both states — the bulk bar REPLACES
        // the tab bar on phones, so TAB_BAR_CLEARANCE's `max()` of the two
        // measured heights already covers a live selection.
        className={`flex-1 overflow-auto ${TAB_BAR_CLEARANCE}`}
      >
        <IssueList
          groups={visibleGroups}
          issueLabelMap={issueLabelMap}
          labels={labelList}
          users={users}
          userMap={userMap}
          onNewIssue={handleNewIssue}
          onIssueClick={(issue) =>
            void navigate({
              to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
              params: {
                teamSlug,
                boardSlug,
                issueIdentifier: issue.identifier,
              },
              // EXP-851: carry the origin, so the sidebar keeps THIS
              // board's list beside the issue.
              search: { from: `board:${boardSlug}` },
            })
          }
          canCreate={permissions.canCreate}
          canMutateIssue={permissions.canMutateIssue}
          canModerate={permissions.isModerator}
          bulkTeamId={team.id}
          selectedIds={selectedIds}
          onSelectedIdsChange={setSelectedIds}
          isLoading={!issuesReady}
          // Members only — the guidance block is meaningless before the
          // viewer's own member row has synced.
          emptyStateExtra={
            permissions.isMember ? (
              <GettingStartedSection
                team={team}
                teamSlug={teamSlug}
              />
            ) : undefined
          }
        />
      </div>

      <CreateIssueDialog
        open={createIssueOpen}
        onOpenChange={(next) => {
          setCreateIssueOpen(next)
          if (!next) setPrefill(undefined)
        }}
        boardId={board.id}
        boardPrefix={board.prefix}
        boardColor={board.color}
        teamId={team.id}
        defaultStatus={defaultStatus}
        prefill={prefill}
        users={users}
      />
    </div>
  )
}
