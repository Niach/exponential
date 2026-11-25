import { useCallback } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { SkeletonRows, useKeyboardInset } from "@exp/ui"
import { HOME_TEST_IDS } from "@exp/domain-contract/home-greeting"
import { CHATS_COPY } from "@exp/domain-contract/chats"
import { useSteerConfig } from "@/hooks/use-steer-config"
import { useSteerDevices } from "@/hooks/use-steer-devices"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { MobilePageHeader } from "@/components/team/mobile-page-header"
import { HomeColumn } from "@/components/home/home-column"
import { HomeDevView, devView, type DevView } from "@/components/home/dev-views"
import { useHomeSeed } from "@/components/home/use-home-seed"
import type { AgentSearch } from "@/lib/launch-seed"
import { pageTitle } from "@/lib/page-title"

const str = (value: unknown): string | undefined =>
  typeof value === `string` && value !== `` ? value : undefined

// EXP-1253: the team's AGENT page is HOME (composer-home.json): the team
// index lands here. EXP-1293 (draft 1): top to bottom, centred,
// the DAWN (FEED-100: the big Exponential mark's top half above the
// composer), S-3 the agent row over the composer ("What do you want to
// build?"; composer-home.json `agentRow`), then, while the team has no board, Getting started. No greeting, no
// history button, no recent list: Chats (the rail) is where past work
// lives. EXP-1316 phones: the column stands at the BOTTOM (the dawn on the
// card, the options row under it, keyboard-aware), Getting started above the
// dawn, no Running list.
//
// The composer is THE launcher: a start is ONE `sessions.send` in this team
// and lands on `/t/$teamSlug/sessions/$sessionId`. A deep link (a native
// client, a notification, an old bookmark) may still seed it through the
// search params, consumed once and stripped with a replace navigation:
//
//   issues  csv of issue ids (chips; 2+ = a batch)
//   device  the machine to pre-pick
//   text    inserted into the empty draft
//   share   EXP-1635: the pending share (`lib/share/pending-share.ts`) lands
//           in the composer, the agent row on this pick; nothing is sent

// The `?dev=` specimens (components/home/dev-views.tsx), DEV-ONLY.

export const Route = createFileRoute(`/t/$teamSlug/agent`)({
  head: () => ({ meta: [{ title: pageTitle(`Agent`) }] }),
  validateSearch: (
    search: Record<string, unknown>
  ): AgentSearch & { from?: string; dev?: DevView; share?: string } => ({
    issues: str(search.issues),
    device: str(search.device),
    text: str(search.text),
    share: str(search.share),
    // The origin a play button names (`lib/detail-origin.ts`); kept so its
    // links stay valid, nothing on this page reads it any more.
    from: str(search.from),
    dev: devView(search.dev),
  }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: AgentRoute,
})

function AgentRoute() {
  const { dev } = Route.useSearch()
  if (dev) return <HomeDevView dev={dev} />
  return <AgentPage />
}

function AgentPage() {
  const { teamSlug } = Route.useParams()
  const search = Route.useSearch()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const { data: authSession } = useSession()
  const { isMember } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  // EXP-790: the prompt box is the mention field, so `@` needs the roster.
  const { users: teamUsers } = useTeamUsers(team?.id)
  const currentUserId = authSession?.user?.id
  // The relay must be configured for a device to pick the start up; the
  // server enforces membership, this only decides what renders.
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)
  const { devices } = useSteerDevices({
    enabled: steerEnabled,
    currentUserId,
    teamId: team?.id,
  })

  const { inset: keyboard } = useKeyboardInset()
  const stripSeedKeys = useCallback(() => {
    void navigate({
      to: `/t/$teamSlug/agent`,
      params: { teamSlug },
      search: search.from ? { from: search.from } : {},
      replace: true,
    })
  }, [navigate, teamSlug, search.from])
  const { seed, onSeedConsumed } = useHomeSeed(search, stripSeedKeys)

  if (!team || !currentUserId) {
    return <SkeletonRows className="mx-auto w-full max-w-2xl px-4 py-6" />
  }

  return (
    <div className="relative flex h-full min-h-0 flex-col" data-testid="agent-page">
      {/* #EXP-1384 (chats.json `shell.header`): phones wear THE page header
          (title `copy.headers.agent`). */}
      <MobilePageHeader
        teamSlug={teamSlug}
        title={CHATS_COPY.headers.agent}
        className="shrink-0 px-4 pb-2 pt-[max(1rem,env(safe-area-inset-top))] md:hidden"
      />
      <div className="relative min-h-0 flex-1 overflow-y-auto">
        {/* EXP-1316 (composer-home.json `phone`): phones pin the dawn + the
            card + its options row to the BOTTOM (above the tab bar, riding
            the keyboard), Getting started above the dawn, no Running list;
            md+ keeps the centred column. */}
        <div
          data-testid={HOME_TEST_IDS.home}
          className={`mx-auto flex min-h-full w-full max-w-2xl flex-col justify-center px-4 py-6 max-md:justify-end ${PAGE_END_CLEARANCE}`}
          style={keyboard > 0 ? { paddingBottom: keyboard } : undefined}
        >
          <HomeColumn
            team={team}
            isMember={isMember}
            steerEnabled={steerEnabled}
            devices={devices}
            users={teamUsers}
            seed={seed}
            onSeedConsumed={onSeedConsumed}
          />
        </div>
      </div>
    </div>
  )
}
