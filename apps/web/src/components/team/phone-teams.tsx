import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { CHATS_COPY } from "@exp/domain-contract/chats"
import { rowBadge } from "@exp/domain-contract/nav-badges"
import { phoneTeamsEmpty, SIDEBAR_TREE_COPY, sidebarTeamOpenKey } from "@exp/domain-contract/sidebar-tree"
import { Button, conceptIcon, FabButton, GlassGroup, Menu, SkeletonRows } from "@exp/ui"
import type { Team } from "@/db/schema"
import { useCrossTeamChatList } from "@/components/chat/cross-team-chat-list"
import { CreateTeamDialog } from "@/components/create-team-dialog"
import { Empty } from "@/components/empty"
import { GettingStartedSection } from "@/components/getting-started/getting-started-section"
import { useMemberTeams } from "@/components/home/use-member-teams"
import { JoinTeamDialog } from "@/components/join-team-dialog"
import { MobilePageHeader } from "@/components/team/mobile-page-header"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { SidebarTreeDensityContext } from "@/components/team/sidebar-folders"
import { sidebarMenuEntries } from "@/components/team/sidebar-menus"
import { sidebarActive, SidebarTeamSections } from "@/components/team/sidebar-tree"
import { useMemberNeedsYouCounts } from "@/hooks/use-app-icon-badge"
import { useChatsList } from "@/hooks/use-session-tree"
import { setSidebarOpenFlag } from "@/hooks/use-sidebar-tree"
import { useTeamMemberships } from "@/hooks/use-team-data"
import { newPagePath } from "@/lib/app-paths"
import { emptyStateCopy } from "@/lib/empty-states"
import type { ChatsListRow } from "@/lib/session-tree"

// #EXP-2109 (sidebar-tree.json `phone`, ×3 with iOS and Android): the phones'
// TEAMS tab = the sidebar's own team trees (`SidebarTeamSections`) at touch
// density in one glass group: teams and folders fold in place, a leaf opens
// its target, a long-press opens the row's menu.

const AddIcon = conceptIcon(`ui-add`)
/** Nothing in the tab is "the open entry". */
const NO_ACTIVE = sidebarActive(`/teams`)

export function PhoneTeams({ me, teamSlug }: { me: string; teamSlug?: string }) {
  const navigate = useNavigate()
  const { teams, teamIds } = useMemberTeams(me)
  const { memberships } = useTeamMemberships(me)
  const owners = useMemo(() => new Set(memberships.filter((m) => m.role === `owner`).map((m) => m.teamId)), [memberships])
  const needsYou = useMemberNeedsYouCounts()
  const { groups } = useChatsList(teamIds)
  const chatRows = useMemo(() => {
    const map = new Map<string, ChatsListRow>()
    for (const group of groups) for (const row of group.rows) map.set(row.sessionId, row)
    return map
  }, [groups])
  // A placed session's count badge = its Chats row's.
  const { list } = useCrossTeamChatList(teamIds, me)
  const sessionBadges = useMemo(() => new Map(list.rows.map((row) => [row.sessionId, rowBadge(row)])), [list.rows])
  const [counts, setCounts] = useState<ReadonlyMap<string, number>>(new Map())
  const onCount = useCallback(
    (teamId: string, count: number) =>
      setCounts((prev) => (prev.get(teamId) === count ? prev : new Map(prev).set(teamId, count))),
    []
  )
  const [createOpen, setCreateOpen] = useState(false)
  const [joinOpen, setJoinOpen] = useState(false)

  // A link to a team: unfolded and scrolled to, once.
  const listRef = useRef<HTMLDivElement>(null)
  const linked = teamSlug ? teams.find((team) => team.slug === teamSlug)?.id : undefined
  useEffect(() => {
    if (!linked || !teamSlug) return
    setSidebarOpenFlag(sidebarTeamOpenKey(linked), true)
    listRef.current?.querySelector(`[data-team-slug="${CSS.escape(teamSlug)}"]`)?.scrollIntoView?.({ block: `nearest` })
    void navigate({ to: `/teams`, search: {}, replace: true })
  }, [linked, teamSlug, navigate])

  const emptyId = phoneTeamsEmpty(teams.map((team) => ({ id: team.id, count: counts.get(team.id) ?? -1 })))
  const empty = teams.find((team) => team.id === emptyId)
  return (
    <div className="h-full overflow-y-auto max-md:h-dvh" data-testid="phone-teams" data-scroll-restoration-id="phone-teams">
      <div className={`mx-auto flex w-full max-w-xl flex-col gap-4 px-4 ${PAGE_END_CLEARANCE}`}>
        <MobilePageHeader
          title={CHATS_COPY.headers.teams}
          trailing={
            <Menu
              align="end"
              aria-label={SIDEBAR_TREE_COPY.teams}
              title={SIDEBAR_TREE_COPY.teams}
              entries={sidebarMenuEntries(`teams`, {}, {
                "new-team": () => setCreateOpen(true),
                "join-team": () => setJoinOpen(true),
              })}
              trigger={
                <FabButton emphasis="primary" className="size-11" aria-label={SIDEBAR_TREE_COPY.teams} data-testid="teams-add">
                  <AddIcon className="size-5" />
                </FabButton>
              }
            />
          }
        />
        {teams.length === 0 ? (
          <SkeletonRows rowHeight={48} />
        ) : (
          <SidebarTreeDensityContext value="touch">
            <GlassGroup ref={listRef} className="divide-y-0 py-1" data-testid="teams-tree">
              <SidebarTeamSections
                teams={teams}
                owners={owners}
                needsYou={needsYou.byTeam}
                chatRows={chatRows}
                me={me}
                active={NO_ACTIVE}
                sessionBadges={sessionBadges}
                onCount={onCount}
              />
            </GlassGroup>
          </SidebarTreeDensityContext>
        )}
        {empty && <EmptyTeam team={empty} owner={owners.has(empty.id)} />}
      </div>
      <CreateTeamDialog open={createOpen} onOpenChange={setCreateOpen} />
      <JoinTeamDialog open={joinOpen} onOpenChange={setJoinOpen} />
    </div>
  )
}

/** `phone.empty`: the only team holds nothing yet: a board is the first step;
 *  the member's progress. */
function EmptyTeam({ team, owner }: { team: Team; owner: boolean }) {
  const navigate = useNavigate()
  return (
    <>
      <Empty id="boards">
        {owner && (
          <Button variant="outline" size="sm" onClick={() => void navigate({ href: newPagePath(team.slug, `board`) })}>
            {emptyStateCopy(`boards`).action}
          </Button>
        )}
      </Empty>
      <GettingStartedSection team={team} teamSlug={team.slug} />
    </>
  )
}
