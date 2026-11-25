import { useCallback, useEffect, useMemo, useState } from "react"
import { createFileRoute, useNavigate, useRouter } from "@tanstack/react-router"
import { MobileDetailHeader, SkeletonRows, useIsMobile, useKeyboardInset } from "@exp/ui"
import { CHATS_COPY } from "@exp/domain-contract/chats"
import { PushedTitle } from "@/components/team/mobile-page-header"
import { HOME_TEST_IDS } from "@exp/domain-contract/home-greeting"
import { useSteerConfig } from "@/hooks/use-steer-config"
import { useSteerDevices } from "@/hooks/use-steer-devices"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { HomeColumn } from "@/components/home/home-column"
import { HomeDevView, devView, type DevView } from "@/components/home/dev-views"
import { useHomeSeed } from "@/components/home/use-home-seed"
import { useMemberTeams } from "@/components/home/use-member-teams"
import type { LaunchComposerHome } from "@/hooks/use-launch-composer"
import type { AgentSearch } from "@/lib/launch-seed"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027 (sidebar-tree.json ROOT `home`): HOME, the agent composer page
// renamed from Agent, TEAM-LESS at every width (#EXP-2109: a phone pushes it
// from New › Chat). The column (components/home): the dawn, the agent row (the viewer's Assistant +
// the team agents of EVERY member team, Temporary session) and the card.
// The Assistant sees every team (its Teams pick, null = all); a team agent's
// chat is its own team; a plain chat carries THE same Teams pick in the "+"
// (at least one team, the first in team order by default, `?team=<id>`
// presets it: a team menu's New › Session). Everything team-scoped in the
// card follows the team the send lands in; the draft stays keyed on the
// default team, so a pick never loses the typing. Seeds: `?issues`,
// `?device`, `?text`, `?share`; the DEV `?dev=` specimens.

const str = (value: unknown): string | undefined =>
  typeof value === `string` && value !== `` ? value : undefined

type HomeSearch = AgentSearch & { dev?: DevView; share?: string; team?: string }

export const Route = createFileRoute(`/_app/home`)({
  head: () => ({ meta: [{ title: pageTitle(`Home`) }] }),
  validateSearch: (search: Record<string, unknown>): HomeSearch => ({
    issues: str(search.issues),
    device: str(search.device),
    text: str(search.text),
    share: str(search.share),
    team: str(search.team),
    dev: devView(search.dev),
  }),
  component: HomeRoute,
})

function HomeRoute() {
  const { dev } = Route.useSearch()
  if (dev) return <HomeDevView dev={dev} />
  return <HomePage />
}

function HomePage() {
  const { user, defaultTeamSlug } = Route.useRouteContext()
  const search = Route.useSearch()
  const navigate = useNavigate()
  const team = useTeamBySlug(defaultTeamSlug)
  const me = user.id
  const isMobile = useIsMobile()
  const router = useRouter()
  const { inset: keyboard } = useKeyboardInset()
  const memberTeams = useMemberTeams(me)
  const steerConfig = useSteerConfig()
  const steerEnabled = Boolean(steerConfig?.enabled)
  const { devices } = useSteerDevices({
    enabled: steerEnabled,
    currentUserId: me,
    listTeamIds: memberTeams.teamIds,
  })

  const stripSeedKeys = useCallback(() => {
    void navigate({ to: `/home`, search: {}, replace: true })
  }, [navigate])
  const { seed, onSeedConsumed } = useHomeSeed(search, stripSeedKeys)
  // `?team=` is one-shot like the seed keys: held here (a fresh object per
  // link), then stripped.
  const [seedTeam, setSeedTeam] = useState<{ id: string } | null>(null)
  useEffect(() => {
    if (!search.team) return
    setSeedTeam({ id: search.team })
    stripSeedKeys()
  }, [search.team, stripSeedKeys])
  const home = useMemo<LaunchComposerHome>(
    () => ({
      teams: memberTeams.teams.map((row) => ({ id: row.id, name: row.name, iconUrl: row.iconUrl ?? null })),
      seedTeam,
    }),
    [memberTeams.teams, seedTeam]
  )

  if (!team) {
    return <SkeletonRows className="mx-auto w-full max-w-2xl px-4 py-6" />
  }

  return (
    <div className="relative flex h-full min-h-0 flex-col" data-testid="agent-page">
      {/* #EXP-2109: a phone pushes it from New › Chat. */}
      {isMobile && (
        <div className="shrink-0 pt-[env(safe-area-inset-top)]">
          <MobileDetailHeader
            title={<PushedTitle>{CHATS_COPY.headers.agent}</PushedTitle>}
            onBack={() => (router.history.canGoBack() ? router.history.back() : void navigate({ to: `/chats` }))}
          />
        </div>
      )}
      <div className="relative min-h-0 flex-1 overflow-y-auto">
        <div
          data-testid={HOME_TEST_IDS.home}
          // EXP-1316: phones pin the column to the BOTTOM, riding the keyboard.
          className="mx-auto flex min-h-full w-full max-w-2xl flex-col justify-center px-4 py-6 max-md:justify-end max-md:pb-[max(1.5rem,env(safe-area-inset-bottom))]"
          style={isMobile && keyboard > 0 ? { paddingBottom: keyboard } : undefined}
        >
          <HomeColumn
            team={team}
            isMember
            steerEnabled={steerEnabled}
            devices={devices}
            users={[]}
            seed={seed}
            onSeedConsumed={onSeedConsumed}
            home={home}
          />
        </div>
      </div>
    </div>
  )
}
