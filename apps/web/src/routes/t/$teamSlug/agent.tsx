import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import {
  Button,
  conceptIcon,
  ExponentialLogo,
  FeedbackStatesSpecimen,
  SkeletonRows,
  THREAD_COPY,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@exp/ui"
import {
  HOME_GEOMETRY,
  HOME_TEST_IDS,
  homeGreeting,
  localWallClock,
} from "@exp/domain-contract/home-greeting"
import { useSteerConfig } from "@/hooks/use-steer-config"
import { SessionsList } from "@/components/agent-shell"
import { GettingStartedSection } from "@/components/getting-started/getting-started-section"
import { LaunchComposer } from "@/components/launch-composer"
import { useAgentsData } from "@/hooks/use-agents-data"
import { useCrossTeamScope } from "@/hooks/use-cross-team-scope"
import { useLaunchComposer } from "@/hooks/use-launch-composer"
import { useOpenSession } from "@/hooks/use-open-session"
import {
  threadIdOfRun,
  useThreadBundle,
  waitForThreadOfRun,
} from "@/hooks/use-threads"
import { useRemoteStart, type RemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import {
  useTeamBoardsWithReady,
  useTeamBySlug,
  useTeamUsers,
} from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { updateWorkTabs } from "@/hooks/use-work-tabs"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import type { CodingSession, Team, User } from "@/db/schema"
import { landNewRun, markNewRunStarting } from "@/lib/work-tabs"
import {
  seedFromSearch,
  type AgentSearch,
  type LaunchSeed,
} from "@/lib/launch-seed"
import { pageTitle } from "@/lib/page-title"
import { isProductionBuild } from "@/lib/production-build"
import {
  setRecentRunsPanelOpen,
  useRecentRunsPanelOpen,
} from "@/lib/recent-runs-panel"

// EXP-923: the history glyph — the ONE concept every client names its run
// history with.
const RecentRunsIcon = conceptIcon(`settings-sessions`)

const str = (value: unknown): string | undefined =>
  typeof value === `string` && value !== `` ? value : undefined
// EXP-1233: `?conflict=1` — the router parses a bare `1` as a NUMBER, so the
// flag is normalised back to the seed's string form here.
const flag = (value: unknown): string | undefined =>
  value === `1` || value === 1 || value === true ? `1` : undefined

// EXP-818: the team's AGENT page — the composer over the caller's sessions
// list (Running, then Past). EXP-825 made that composer THE launcher: issues,
// actions and a plain chat all start from it.
//
// EXP-1253 (EXP-1259 D1): it is HOME (composer-home.json): the team index
// lands here, and the web New run tab (EXP-1255) hosts it. Top to bottom:
// the headline (the disc mark + the time-of-day greeting, computed once per
// mount) while no subject is picked (a subject's own headline leads instead),
// the composer ("What do you want to build?") and its options line, then,
// while the team has no board, the Getting started block. Nothing else sits
// under the options line: recent sessions live behind the history button
// (the Agent › Recent panel).
//
// EXP-1019: a play button no longer travels here for work it can already
// name — a seed carrying a SUBJECT opens the same composer as a dialog over
// the surface the click came from (`components/launch-dialog/launch-dialog.tsx`,
// `hooks/use-open-composer.ts`). This route stays the home of a plain CHAT,
// and it keeps honouring the full seed in its search params, because a deep
// link from a native client, a notification or an old bookmark still arrives
// that way:
//
//   issues  csv of issue ids (chips; 2+ = a batch)
//   action  an action id (`builtin:fix-conflicts`, `builtin:create-action`,
//           or a row id) — wins over `issues` when both arrive
//   pr      an issue id linked to the open PR a `pr` input opens on
//   device  the machine to pre-pick
//   text    inserted into the empty draft (a suggestion's description)
//   icon    a curated icon name seeding Create action's `icon` input
//   conflict `1` when a refused merge opened the Fix merge conflicts builtin
//           (EXP-1233) — the composer's card says so
//   from    where the launch came from (`lib/detail-origin.ts`) — the sidebar
//           keeps that list beside the composer, and the run it starts
//           inherits it (an issue origin lands on the issue's session route)
//
// The seed is ONE-SHOT: consumed by the composer, then stripped with a
// replace navigation (the `actions.tsx` `?editAction=` pattern), so a
// back/refresh never re-preselects. Clicking a session opens
// `/t/$teamSlug/sessions/$sessionId` (EXP-851: full width, with this list in
// the sidebar beside it).

// EXP-1259 (DEV-ONLY): `?dev=feedback-states` swaps the page for the six
// feedback kinds side by side (`@exp/ui` `FeedbackStatesSpecimen`, the
// styleguide's board), for the view catalog's `feedback-states` capture:
// the kinds never show together in the app. Never parsed in a production
// build (`isProductionBuild`).
type DevView = `feedback-states`
const devView = (value: unknown): DevView | undefined =>
  !isProductionBuild && value === `feedback-states` ? value : undefined

export const Route = createFileRoute(`/t/$teamSlug/agent`)({
  head: () => ({ meta: [{ title: pageTitle(`Agent`) }] }),
  validateSearch: (
    search: Record<string, unknown>
  ): AgentSearch & { from?: string; thread?: string; dev?: DevView } => ({
    issues: str(search.issues),
    action: str(search.action),
    pr: str(search.pr),
    device: str(search.device),
    text: str(search.text),
    icon: str(search.icon),
    conflict: flag(search.conflict),
    from: str(search.from),
    // EXP-1284: "Start a session here" — every start from this page goes INTO
    // that thread. Sticky like `from` (never stripped with the seed).
    thread: str(search.thread),
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
  if (dev === `feedback-states`) {
    return (
      <div className={`h-full min-h-0 overflow-y-auto p-6 ${TAB_BAR_CLEARANCE}`}>
        <FeedbackStatesSpecimen />
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
  // Steer tickets require team membership and a configured relay; the server
  // enforces both at mint time, this only decides what renders.
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)

  // EXP-1255: a session started here LANDS → the New session tab waiting on
  // THAT START (keyed by its start id, never by the window) turns into the
  // session's tab, in place. EXP-1284: every start joins a THREAD, so it
  // lands as that thread's tab and its page opens — the session's item
  // usually syncs with its row; a start that names `?thread=` knows it up
  // front.
  const teamIdForTabs = team?.id
  const requestedThread = search.thread ?? null
  const fromRef = useRef(search.from)
  fromRef.current = search.from
  const openSessionDirect = useOpenSession()
  const onSent = useCallback(
    ({ startId }: { startId: string }) => {
      if (!teamIdForTabs) return
      updateWorkTabs(teamIdForTabs, (state) => markNewRunStarting(state, startId))
    },
    [teamIdForTabs]
  )
  const onOpened = useCallback(
    (run: CodingSession, start: { startId: string | null }): boolean => {
      if (!teamIdForTabs) return false
      const land = (threadId: string | null) => {
        updateWorkTabs(teamIdForTabs, (state) =>
          landNewRun(
            state,
            { runId: run.id, issueId: run.issueId, threadId },
            start.startId
          )
        )
        if (!threadId) {
          openSessionDirect(run)
          return
        }
        void navigate({
          to: `/t/$teamSlug/threads/$threadId`,
          params: { teamSlug, threadId },
          search: fromRef.current ? { from: fromRef.current } : {},
        })
      }
      const known = threadIdOfRun(run.id) ?? requestedThread
      if (known) {
        land(known)
        return true
      }
      void waitForThreadOfRun(run.id).then(land)
      return true
    },
    [teamIdForTabs, requestedThread, navigate, teamSlug, openSessionDirect]
  )
  const remote = useRemoteStart({
    enabled: steerEnabled,
    currentUserId,
    teamId: team?.id,
    onOpened,
    onSent,
    threadId: requestedThread,
  })

  // The one-shot seed: held here after the URL keys are stripped, handed to
  // the composer, cleared once it reports consumption.
  const urlSeed = useMemo(
    () => seedFromSearch(search),
    // Field-wise: the search object's identity is the router's business.
    [
      search.issues,
      search.action,
      search.pr,
      search.device,
      search.text,
      search.icon,
      search.conflict,
    ]
  )
  const [seed, setSeed] = useState<LaunchSeed | null>(null)
  // EXP-862: `?action=` MIRRORS the composer's picked action while it holds
  // one, so the sidebar's pinned row for that action reads as active (and the
  // Agent nav entry does not). A mirror write must never look like a fresh
  // seed, hence the latch: the effect below ignores a search that is exactly
  // the action we just wrote.
  const mirroredActionRef = useRef<string | null>(null)
  // The URL keys the mirror compares against and carries along, read through
  // a ref so `mirrorAction` keeps ONE identity across search changes: the
  // composer's effect below re-fires on the action it holds, never on the
  // router's search object — otherwise a fresh `?issues=` seed would trip a
  // mirror write of the old action first (child effects run before the
  // parent's), navigating the seed away for one frame.
  const searchRef = useRef(search)
  searchRef.current = search
  // Set while a fresh seed is on its way into the composer: the mirror is
  // held off until the composer reports consumption, then applied ONCE from
  // the action it settled on — so the seed's clearing navigation is never
  // raced by a mirror write, and the pinned row still lights up afterwards.
  const seedPendingRef = useRef(false)
  const wantedActionRef = useRef<string | null>(null)

  const mirrorAction = useCallback(
    (actionId: string | null) => {
      mirroredActionRef.current = actionId
      const current = searchRef.current
      // Already what the URL says (the common case on every re-render).
      if ((current.action ?? null) === actionId) return
      void navigate({
        to: `/t/$teamSlug/agent`,
        params: { teamSlug },
        search: {
          ...(current.from ? { from: current.from } : {}),
          ...(current.thread ? { thread: current.thread } : {}),
          ...(actionId ? { action: actionId } : {}),
        },
        replace: true,
      })
    },
    [navigate, teamSlug]
  )

  const onActionChange = useCallback(
    (actionId: string | null) => {
      wantedActionRef.current = actionId
      if (seedPendingRef.current) return
      mirrorAction(actionId)
    },
    [mirrorAction]
  )

  const onSeedConsumed = useCallback(() => {
    setSeed(null)
    seedPendingRef.current = false
    mirrorAction(wantedActionRef.current)
  }, [mirrorAction])

  useEffect(() => {
    if (!urlSeed) return
    const mirrorOnly =
      urlSeed.actionId != null &&
      urlSeed.actionId === mirroredActionRef.current &&
      urlSeed.issueIds.length === 0 &&
      !urlSeed.deviceId &&
      !urlSeed.prIssueId &&
      !urlSeed.text &&
      !urlSeed.icon &&
      !urlSeed.conflict
    if (mirrorOnly) return
    seedPendingRef.current = true
    setSeed(urlSeed)
    void navigate({
      to: `/t/$teamSlug/agent`,
      params: { teamSlug },
      // EXP-851: the seed is one-shot, the ORIGIN is not — it decides where
      // the launched run lands and which list the sidebar keeps.
      search: {
        ...(search.from ? { from: search.from } : {}),
        ...(search.thread ? { thread: search.thread } : {}),
      },
      replace: true,
    })
  }, [urlSeed, navigate, teamSlug, search.from, search.thread])

  // EXP-862: with nothing running, the composer is the whole page — it
  // centres in the column instead of hanging off the top edge (the desktop's
  // `min_h_full` + centred chat column). EXP-923: on md+ there is no list at
  // all, so it centres there unconditionally.
  // EXP-1186: the phone's Running list reads every member team (the
  // composer keeps the active one); md+ = the active team.
  const scope = useCrossTeamScope(team)
  const { running } = useAgentsData(scope.teamIds, currentUserId)
  const listEmpty = running.length === 0
  // EXP-923: the Recent panel is a disclosure on THIS page. EXP-1246: it
  // survives the runs it opens (`?from=agent:recent`) and shuts anywhere else
  // (`useRecentRunsPanelRouteGuard`, mounted by the sidebar).
  const recentOpen = useRecentRunsPanelOpen()

  if (!team || !currentUserId) {
    return <SkeletonRows className="mx-auto w-full max-w-2xl px-4 py-6" />
  }

  // EXP-851: ONE column on every breakpoint, ONE scroller — the composer on
  // top, Running/Past under it. The side column is gone: an open run's list
  // lives in the sidebar's list nav now, so the page never nests a second
  // list beside itself (and never a second vertical scrollbar either).
  return (
    <div
      className="relative flex h-full min-h-0 flex-col"
      data-testid="agent-page"
    >
      {/* EXP-923: the page's ONE history control — it slides the sidebar's
          Recent panel in beside the compact rail (md+; the phone reaches the
          same list through the topbar's sheet). EXP-1119: hidden while the
          panel is up — the panel's own back row shuts it. */}
      {!recentOpen && (
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="absolute top-3 left-3 z-10 hidden size-8 text-muted-foreground hover:text-foreground md:flex"
              aria-label="Recent sessions"
              data-testid="recent-runs-toggle"
              onClick={() => {
                setRecentRunsPanelOpen(true)
                // This button unmounts while the panel is up: focus the
                // panel's back row, which is the way out.
                requestAnimationFrame(() =>
                  document
                    .querySelector<HTMLElement>(`[aria-label="Back to Agent"]`)
                    ?.focus(),
                )
              }}
            >
              <RecentRunsIcon className="size-4" />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="right">Recent sessions</TooltipContent>
        </Tooltip>
      )}
      <div className="relative min-h-0 flex-1 overflow-y-auto">
        <div
          data-testid={HOME_TEST_IDS.home}
          className={`mx-auto flex w-full max-w-2xl flex-col px-4 py-6 md:min-h-full md:justify-center ${
            listEmpty ? `min-h-full justify-center` : ``
          } ${TAB_BAR_CLEARANCE}`}
        >
          {search.thread && (
            <StartInThreadLine
              teamId={team.id}
              currentUserId={currentUserId}
              threadId={search.thread}
              onClear={() =>
                void navigate({
                  to: `/t/$teamSlug/agent`,
                  params: { teamSlug },
                  search: search.from ? { from: search.from } : {},
                  replace: true,
                })
              }
            />
          )}
          <HomeColumn
            team={team}
            userName={authSession?.user?.name ?? null}
            isMember={isMember}
            steerEnabled={steerEnabled}
            remote={remote}
            users={teamUsers}
            seed={seed}
            onSeedConsumed={onSeedConsumed}
            onActionChange={onActionChange}
          />
          {/* EXP-923: PHONE ONLY. On md+ what is running is a sidebar
              section, so the page is the composer alone; the phone keeps its
              Running list under the composer, inside the page's ONE scroller
              so it never traps a nested one. */}
          <SessionsList
            teamId={team.id}
            teams={scope.teams}
            grouped={scope.grouped}
            currentUserId={currentUserId}
            activeSessionId={null}
            origin={{ kind: `agent` }}
            scroll={false}
            // EXP-862: the Agent page ALWAYS draws the Running band, empty or
            // not.
            showWhenEmpty
            // The page's own container already reserves the tab bar's
            // clearance — the list must not add a second one inside it.
            className="mt-6 p-0 max-md:pb-0 md:hidden"
          />
        </div>
      </div>
    </div>
  )
}

/** EXP-1284: "Start a session here" — the thread every start from this page
 *  goes into, named above the composer, with the way out. */
function StartInThreadLine({
  teamId,
  currentUserId,
  threadId,
  onClear,
}: {
  teamId: string
  currentUserId: string
  threadId: string
  onClear: () => void
}) {
  const { bundle } = useThreadBundle(teamId, currentUserId, threadId)
  if (!bundle) return null
  return (
    <div
      data-testid="start-in-thread"
      className="mb-4 flex min-w-0 items-center justify-center gap-1.5 text-xs text-muted-foreground"
    >
      <span className="shrink-0">Starting in</span>
      <span className="min-w-0 truncate font-medium text-foreground">{bundle.view.title}</span>
      <span className="shrink-0">{THREAD_COPY.word}</span>
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label="Start outside the thread"
        className="shrink-0"
        onClick={onClear}
      >
        <ClearThreadIcon />
      </Button>
    </div>
  )
}

const ClearThreadIcon = conceptIcon(`ui-close`)

/** EXP-1253: the home column under the page chrome: the headline + composer
 *  (or the relay-less line), then Getting started while the team has no
 *  board; nothing else. The greeting is computed ONCE per mount. */
export function HomeColumn({
  team,
  userName,
  isMember,
  steerEnabled,
  remote,
  users,
  seed,
  onSeedConsumed,
  onActionChange,
}: {
  team: Team
  userName: string | null
  isMember: boolean
  steerEnabled: boolean
  remote: RemoteStart
  users: User[]
  seed: LaunchSeed | null
  onSeedConsumed: () => void
  onActionChange: (actionId: string | null) => void
}) {
  const [greeting] = useState(() => homeGreeting(localWallClock(new Date()), userName))
  const { boards, boardsReady } = useTeamBoardsWithReady(team.id)
  return (
    <>
      {steerEnabled ? (
        // Keyed by team: the composer's repo pick and seed latches are
        // one-shot per mount, so a /t/a/agent → /t/b/agent navigation
        // remounts it instead of carrying team A's repo into team B.
        <ComposerPane
          key={team.id}
          teamId={team.id}
          greeting={greeting}
          remote={remote}
          users={users}
          seed={seed}
          onSeedConsumed={onSeedConsumed}
          onActionChange={onActionChange}
        />
      ) : (
        <>
          <HomeHeadline greeting={greeting} />
          <p
            className="text-center text-sm text-muted-foreground"
            style={{ marginTop: HOME_GEOMETRY.headlineToCard }}
          >
            Live steering is unavailable on this instance.
          </p>
        </>
      )}
      {/* EXP-1270: a board-less team's Getting started block (members only:
          meaningless before the viewer's own member row has synced). */}
      {isMember && boardsReady && boards.length === 0 && (
        <div style={{ marginTop: HOME_GEOMETRY.composerToGettingStarted }}>
          <GettingStartedSection team={team} teamSlug={team.slug} />
        </div>
      )}
    </>
  )
}

/** The disc mark + ONE line of Inter semibold (composer-home.json
 *  `geometry`; phones `markPhone`/`headlinePhone`). */
function HomeHeadline({ greeting }: { greeting: string }) {
  const { mark, markPhone, gap, headline, headlinePhone, headlineWeight } = HOME_GEOMETRY
  return (
    <div className="flex items-center justify-center" style={{ gap }}>
      <span
        aria-hidden
        data-testid={HOME_TEST_IDS.mark}
        className="flex size-[var(--home-mark-phone)] shrink-0 text-foreground md:size-[var(--home-mark)]"
        style={{
          [`--home-mark` as string]: `${mark}px`,
          [`--home-mark-phone` as string]: `${markPhone}px`,
        }}
      >
        <ExponentialLogo variant="light" size={mark} className="size-full" />
      </span>
      <h1
        data-testid={HOME_TEST_IDS.greeting}
        className="min-w-0 truncate text-[length:var(--home-headline-phone)] leading-tight tracking-tight text-foreground md:text-[length:var(--home-headline)]"
        style={{
          fontWeight: headlineWeight,
          [`--home-headline` as string]: `${headline}px`,
          [`--home-headline-phone` as string]: `${headlinePhone}px`,
        }}
      >
        {greeting}
      </h1>
    </div>
  )
}

/** Mounted only with steering on: the hook wires the synced collections and
 * the options cluster, so it never runs for a member of a relay-less
 * instance. */
function ComposerPane({
  teamId,
  greeting,
  remote,
  users,
  seed,
  onSeedConsumed,
  onActionChange,
}: {
  teamId: string
  greeting: string
  remote: RemoteStart
  users: User[]
  seed: LaunchSeed | null
  onSeedConsumed: () => void
  /** EXP-862: the action the composer holds right now (null = none) — the
   *  route mirrors it into `?action=` for the pinned row's highlight. */
  onActionChange: (actionId: string | null) => void
}) {
  const model = useLaunchComposer({ teamId, remote, seed, onSeedConsumed })
  const subject = model.subject
  const activeActionId = subject?.kind === `action` ? subject.id : null
  useEffect(() => {
    onActionChange(activeActionId)
  }, [activeActionId, onActionChange])
  return (
    <>
      {/* EXP-1253: the greeting leads while nothing is picked; a picked
          subject's own headline (inside the composer) leads instead. */}
      {subject === null && <HomeHeadline greeting={greeting} />}
      <div style={subject === null ? { marginTop: HOME_GEOMETRY.headlineToCard } : undefined}>
        <LaunchComposer model={model} users={users} variant="home" />
      </div>
    </>
  )
}
