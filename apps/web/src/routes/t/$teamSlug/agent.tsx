import { useCallback, useEffect, useMemo, useState, type CSSProperties } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import {
  CodeBlockSpecimen,
  ExponentialLogo,
  FeedbackStatesSpecimen,
  SkeletonRows,
  TraceSpecimen,
  AskDockSpecimen,
  DockCardSpecimen,
  DeviceReadinessSpecimen,
  PlanCardSpecimen,
  ChatListSpecimen,
  ChatSelectionSpecimen,
  ChatMenusSpecimen,
  ChatTranscriptSpecimen,
  PhoneTabBarSpecimen,
  PhoneBoardMenuSpecimen,
  PhonePageHeaderSpecimen,
  useKeyboardInset,
} from "@exp/ui"
import { HOME_GEOMETRY, HOME_TEST_IDS } from "@exp/domain-contract/home-greeting"
import { CHATS_COPY } from "@exp/domain-contract/chats"
import { useSteerConfig } from "@/hooks/use-steer-config"
import { GettingStartedSection } from "@/components/getting-started/getting-started-section"
import { LaunchComposer } from "@/components/launch-composer"
import { useLaunchComposer } from "@/hooks/use-launch-composer"
import { useEnsureAssistant } from "@/hooks/use-ensure-assistant"
import { useSteerDevices } from "@/hooks/use-steer-devices"
import { useSession } from "@/hooks/use-session"
import {
  useTeamBoardsWithReady,
  useTeamBySlug,
  useTeamUsers,
} from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { MobilePageHeader } from "@/components/team/mobile-page-header"
import type { Team, User } from "@/db/schema"
import type { SteerDevice } from "@/lib/steer-devices"
import {
  seedFromSearch,
  type AgentSearch,
  type LaunchSeed,
} from "@/lib/launch-seed"
import { pageTitle } from "@/lib/page-title"
import { isProductionBuild } from "@/lib/production-build"

const str = (value: unknown): string | undefined =>
  typeof value === `string` && value !== `` ? value : undefined

// EXP-1253: the team's AGENT page is HOME (composer-home.json): the team
// index lands here. EXP-1293 (Threads v2, draft 1): top to bottom, centred,
// the DAWN (FEED-100: the big Exponential mark's top half above the
// composer), S-3 the agent row over the composer ("What do you want to
// build?"; composer-home.json `agentRow`), then, while the team has no board, Getting started. No greeting, no
// history button, no recent list: Threads (the rail) is where past work
// lives. EXP-1316 phones: the column stands at the BOTTOM (the dawn on the
// card, the options row under it, keyboard-aware), Getting started above the
// dawn, no Running list.
//
// The composer is THE launcher: a start is ONE `sessions.send` in this team
// and lands on `/t/$teamSlug/threads/$threadId`. A deep link (a native
// client, a notification, an old bookmark) may still seed it through the
// search params, consumed once and stripped with a replace navigation:
//
//   issues  csv of issue ids (chips; 2+ = a batch)
//   device  the machine to pre-pick
//   text    inserted into the empty draft

// EXP-1259 (DEV-ONLY): `?dev=feedback-states` swaps the page for the six
// feedback kinds side by side, for the view catalog's `feedback-states`
// capture; EXP-1324: `?dev=trace` / `?dev=code-block` the Trace and
// CodeBlock specimens (`?dev=ask-dock` THE ask dock's, FEED-160 `?dev=device-readiness` its specimen, EXP-1280 `?dev=plan-card`
// THE rich plan card's; thread 58cb32df `?dev=chat-list` / `chat-menus` /
// `chat-transcript` / `phone-tab-bar`, #EXP-1384 `phone-board-menu` /
// `phone-page-header`, #EXP-1388 `chat-selection` the chats.json specimens) (identical
// ×4) for theirs. Never parsed in a production build (`isProductionBuild`).
const DEV_VIEWS = [
  `feedback-states`,
  `trace`,
  `code-block`,
  `ask-dock`,
  `dock-card`,
  `device-readiness`,
  `plan-card`,
  `chat-list`,
  `chat-menus`,
  `chat-selection`,
  `chat-transcript`,
  `phone-tab-bar`,
  `phone-board-menu`,
  `phone-page-header`,
] as const
type DevView = (typeof DEV_VIEWS)[number]
const devView = (value: unknown): DevView | undefined =>
  !isProductionBuild && DEV_VIEWS.includes(value as DevView)
    ? (value as DevView)
    : undefined

export const Route = createFileRoute(`/t/$teamSlug/agent`)({
  head: () => ({ meta: [{ title: pageTitle(`Agent`) }] }),
  validateSearch: (
    search: Record<string, unknown>
  ): AgentSearch & { from?: string; dev?: DevView } => ({
    issues: str(search.issues),
    device: str(search.device),
    text: str(search.text),
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
  if (dev) {
    return (
      <div className={`h-full min-h-0 overflow-y-auto p-6 ${TAB_BAR_CLEARANCE}`}>
        {dev === `trace` ? (
          <TraceSpecimen />
        ) : dev === `ask-dock` ? (
          <AskDockSpecimen />
        ) : dev === `dock-card` ? (
          <DockCardSpecimen />
        ) : dev === `device-readiness` ? (
          <DeviceReadinessSpecimen />
        ) : dev === `plan-card` ? (
          <PlanCardSpecimen />
        ) : dev === `chat-list` ? (
          <ChatListSpecimen />
        ) : dev === `chat-menus` ? (
          <ChatMenusSpecimen />
        ) : dev === `chat-selection` ? (
          <ChatSelectionSpecimen />
        ) : dev === `chat-transcript` ? (
          <ChatTranscriptSpecimen />
        ) : dev === `phone-tab-bar` ? (
          <PhoneTabBarSpecimen />
        ) : dev === `phone-board-menu` ? (
          <PhoneBoardMenuSpecimen />
        ) : dev === `phone-page-header` ? (
          <PhonePageHeaderSpecimen />
        ) : dev === `code-block` ? (
          <CodeBlockSpecimen />
        ) : (
          <FeedbackStatesSpecimen />
        )}
      </div>
    )
  }
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

  // The one-shot seed: held here after the URL keys are stripped, handed to
  // the composer, cleared once it reports consumption.
  const urlSeed = useMemo(
    () => seedFromSearch(search),
    // Field-wise: the search object's identity is the router's business.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [search.issues, search.device, search.text]
  )
  const [seed, setSeed] = useState<LaunchSeed | null>(null)
  // The layout viewport does not shrink under a phone keyboard: the pinned
  // card rides above it.
  const { inset: keyboard } = useKeyboardInset()
  const onSeedConsumed = useCallback(() => setSeed(null), [])
  useEffect(() => {
    if (!urlSeed) return
    setSeed(urlSeed)
    void navigate({
      to: `/t/$teamSlug/agent`,
      params: { teamSlug },
      search: search.from ? { from: search.from } : {},
      replace: true,
    })
  }, [urlSeed, navigate, teamSlug, search.from])

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
          className={`mx-auto flex min-h-full w-full max-w-2xl flex-col justify-center px-4 py-6 max-md:justify-end ${TAB_BAR_CLEARANCE}`}
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

/** The home column: the dawn + the composer (or the relay-less line), then
 *  Getting started while the team has no board; nothing else. */
export function HomeColumn({
  team,
  isMember,
  steerEnabled,
  devices,
  users,
  seed,
  onSeedConsumed,
}: {
  team: Team
  isMember: boolean
  steerEnabled: boolean
  devices: SteerDevice[] | null
  users: User[]
  seed: LaunchSeed | null
  onSeedConsumed: () => void
}) {
  const { boards, boardsReady } = useTeamBoardsWithReady(team.id)
  const gettingStarted =
    isMember && boardsReady && boards.length === 0 ? (
      <GettingStartedSection team={team} teamSlug={team.slug} />
    ) : null
  return (
    <div className="flex flex-col">
      <HomeDawn />
      {steerEnabled ? (
        // Keyed by team: the composer's repo pick and seed latches are
        // one-shot per mount, so a /t/a/agent → /t/b/agent navigation
        // remounts it instead of carrying team A's repo into team B.
        <ComposerPane
          key={team.id}
          teamId={team.id}
          devices={devices}
          users={users}
          seed={seed}
          onSeedConsumed={onSeedConsumed}
        />
      ) : (
        <p className="text-center text-sm text-muted-foreground">
          Live steering is unavailable on this instance.
        </p>
      )}
      {/* EXP-1270: a board-less team's Getting started block (members only:
          meaningless before the viewer's own member row has synced). */}
      {/* md+: under the composer. Phones: ABOVE the dawn (the card is
          pinned low), the same gap under it. */}
      {gettingStarted && (
        <div
          className="max-md:order-first max-md:mb-[var(--home-gap)]"
          style={
            {
              marginTop: HOME_GEOMETRY.composerToGettingStarted,
              "--home-gap": `${HOME_GEOMETRY.composerToGettingStarted}px`,
            } as CSSProperties
          }
        >
          {gettingStarted}
        </div>
      )}
    </div>
  )
}

/** FEED-100: the DAWN, the big brand mark's top HALF rising over the
 *  composer (a21db0ce4): a clip of half the mark's height with the logo at
 *  its top, FADING toward the card it stands on (EXP-1314: solid for its top
 *  third, no gap). The IDE's `dawn` on every client: 440px at most, ~69% of
 *  the column (the IDE's mark-to-card ratio), 2.5% ink; phones = the natives'
 *  240pt mark (iOS `HomeDawn`, Android `DAWN_MARK`). Decoration only. */
function HomeDawn() {
  return (
    <div
      aria-hidden
      data-testid={HOME_TEST_IDS.mark}
      className="pointer-events-none mx-auto aspect-[2/1] w-[min(440px,69%)] max-md:w-[240px] shrink-0 overflow-hidden text-foreground opacity-[0.025] [mask-image:linear-gradient(black_33%,transparent)] select-none"
    >
      <ExponentialLogo variant="light" size={440} className="h-auto w-full" />
    </div>
  )
}

/** Mounted only with steering on: the hook wires the synced collections and
 *  the options cluster. */
function ComposerPane({
  teamId,
  devices,
  users,
  seed,
  onSeedConsumed,
}: {
  teamId: string
  devices: SteerDevice[] | null
  users: User[]
  seed: LaunchSeed | null
  onSeedConsumed: () => void
}) {
  const { data: authSession } = useSession()
  // S-3: the viewer's Assistant exists before the agent row lists it.
  useEnsureAssistant(teamId)
  const model = useLaunchComposer({
    teamId,
    devices,
    seed,
    onSeedConsumed,
    // S-3 (composer-home.json `agentRow`): the New-chat composer picks its
    // recipient, the viewer's Assistant preselected.
    agentRow: true,
    me: authSession?.user?.id ?? null,
  })
  return <LaunchComposer model={model} users={users} variant="home" />
}

