import { useCallback, useMemo } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { NEEDS_YOU_COPY, type NeedsYouRow } from "@exp/domain-contract/needs-you"
import { FAB_CLEARANCE_CLASS, NeedsYouList, SkeletonRows, TeamAvatar } from "@exp/ui"
import { NeedsYouStuckActions, useNeedsYouPress } from "@/components/needs-you-stuck-actions"
import { useMemberTeams } from "@/components/home/use-member-teams"
import { useCrossTeamNeedsYou, useOpenNeedsYouIn, type CrossTeamNeedsYouRow } from "@/hooks/use-needs-you"
import { useTeamMemberships } from "@/hooks/use-team-data"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027 (sidebar-tree.json ROOT `needs-you`): the TEAM-LESS Needs you page
// of web md+ (#FEED-191 needs-you.json, the content card, no header): every
// member team's plans, questions and ready PRs, each row opening in its own
// team (the viewer's Assistant's in the default team). Phones keep the team's
// page (the `_app` layout forwards them).

export const Route = createFileRoute(`/_app/needs-you`)({
  head: () => ({ meta: [{ title: pageTitle(NEEDS_YOU_COPY.entry) }] }),
  component: NeedsYouRoute,
})

function NeedsYouRoute() {
  const { user, defaultTeamSlug } = Route.useRouteContext()
  const me = user.id
  const memberTeams = useMemberTeams(me)
  const { memberships } = useTeamMemberships(me)
  const owned = useMemo(
    () => new Set(memberships.filter((row) => row.role === `owner`).map((row) => row.teamId)),
    [memberships]
  )
  const ownerOf = useCallback((teamId: string) => owned.has(teamId), [owned])
  const { rows, isReady } = useCrossTeamNeedsYou(memberTeams.teamIds, me, ownerOf)
  const openIn = useOpenNeedsYouIn(me)
  const open = useCallback(
    (row: NeedsYouRow) => {
      const teamId = (row as CrossTeamNeedsYouRow).teamId
      openIn(row, (teamId ? memberTeams.byId.get(teamId)?.slug : undefined) ?? defaultTeamSlug)
    },
    [openIn, memberTeams.byId, defaultTeamSlug]
  )
  // #EXP-1710: an `approval` ready PR row's press = Needs approval.
  const press = useNeedsYouPress(open)
  return (
    <div className="h-full overflow-y-auto">
      <div className={`mx-auto flex w-full max-w-3xl flex-col gap-3 px-4 py-4 ${FAB_CLEARANCE_CLASS}`}>
        {memberTeams.teamIds.length === 0 || (!isReady && rows.length === 0) ? (
          <SkeletonRows />
        ) : (
          <NeedsYouList
            rows={rows}
            onOpen={press.press}
            actions={(row) => <NeedsYouStuckActions row={row} />}
            teamOf={(row) => {
              const team = memberTeams.byId.get((row as CrossTeamNeedsYouRow).teamId ?? ``)
              return team ? <TeamAvatar iconUrl={team.iconUrl} size={14} /> : null
            }}
          />
        )}
        {press.dialog}
      </div>
    </div>
  )
}
