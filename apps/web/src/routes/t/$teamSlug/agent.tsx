import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import {
  Button,
  conceptIcon,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@exp/ui"
import { useSteerConfig } from "@/components/agent-session"
import { SessionsList } from "@/components/agent-shell"
import { LaunchComposer } from "@/components/launch-composer"
import { useAgentsData } from "@/hooks/use-agents-data"
import { useLaunchComposer } from "@/hooks/use-launch-composer"
import { useRemoteStart, type RemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import type { User } from "@/db/schema"
import {
  seedFromSearch,
  type AgentSearch,
  type LaunchSeed,
} from "@/lib/launch-seed"
import { pageTitle } from "@/lib/page-title"
import {
  setRecentRunsPanelOpen,
  toggleRecentRunsPanel,
  useRecentRunsPanelOpen,
} from "@/lib/recent-runs-panel"

// EXP-923: the history glyph — the ONE concept every client names its run
// history with.
const RecentRunsIcon = conceptIcon(`settings-sessions`)

const str = (value: unknown): string | undefined =>
  typeof value === `string` && value !== `` ? value : undefined

// EXP-818: the team's AGENT page — the composer over the caller's sessions
// list (Running, then Past). EXP-825 made that composer THE
// launcher: issues, actions and a plain chat all start here (the three-tab
// dialog and the create-action dialog are gone), so every play button in the
// app navigates to this route with a preselection in the search params:
//
//   issues  csv of issue ids (chips; 2+ = a batch)
//   action  an action id (`builtin:fix-conflicts`, `builtin:create-action`,
//           or a row id) — wins over `issues` when both arrive
//   pr      an issue id linked to the open PR a `pr` input opens on
//   workflow  the draft workflow a `builtin:plan-workflow` run plans (EXP-981)
//   device  the machine to pre-pick
//   text    inserted into the empty draft (a suggestion's description)
//   icon    a curated icon name seeding Create action's `icon` input
//   from    where the launch came from (`lib/detail-origin.ts`) — the sidebar
//           keeps that list beside the composer, and the run it starts
//           inherits it (an issue origin lands on the issue's session route)
//
// The seed is ONE-SHOT: consumed by the composer, then stripped with a
// replace navigation (the `actions.tsx` `?editAction=` pattern), so a
// back/refresh never re-preselects. Clicking a session opens
// `/t/$teamSlug/sessions/$sessionId` (EXP-851: full width, with this list in
// the sidebar beside it).

export const Route = createFileRoute(`/t/$teamSlug/agent`)({
  head: () => ({ meta: [{ title: pageTitle(`Agent`) }] }),
  validateSearch: (
    search: Record<string, unknown>
  ): AgentSearch & { from?: string } => ({
    issues: str(search.issues),
    action: str(search.action),
    pr: str(search.pr),
    workflow: str(search.workflow),
    device: str(search.device),
    text: str(search.text),
    icon: str(search.icon),
    from: str(search.from),
  }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: AgentPage,
})

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

  const remote = useRemoteStart({
    enabled: steerEnabled,
    currentUserId,
    teamId: team?.id,
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
      search.workflow,
      search.device,
      search.text,
      search.icon,
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
      !urlSeed.workflowId &&
      !urlSeed.text &&
      !urlSeed.icon
    if (mirrorOnly) return
    seedPendingRef.current = true
    setSeed(urlSeed)
    void navigate({
      to: `/t/$teamSlug/agent`,
      params: { teamSlug },
      // EXP-851: the seed is one-shot, the ORIGIN is not — it decides where
      // the launched run lands and which list the sidebar keeps.
      search: search.from ? { from: search.from } : {},
      replace: true,
    })
  }, [urlSeed, navigate, teamSlug, search.from])

  // EXP-862: with nothing running, the composer is the whole page — it
  // centres in the column instead of hanging off the top edge (the desktop's
  // `min_h_full` + centred chat column). EXP-923: on md+ there is no list at
  // all, so it centres there unconditionally.
  const { running } = useAgentsData(team?.id, currentUserId)
  const listEmpty = running.length === 0
  // EXP-923: the Recent panel is a disclosure on THIS page — leaving it (or
  // switching team) always shuts it again.
  const recentOpen = useRecentRunsPanelOpen()
  useEffect(() => () => setRecentRunsPanelOpen(false), [])

  if (!team || !currentUserId) {
    return <div className="p-6 text-sm text-muted-foreground">Loading…</div>
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
          same list through the topbar's sheet). */}
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="absolute top-3 left-3 z-10 hidden size-8 text-muted-foreground hover:text-foreground md:flex"
            aria-label="Recent runs"
            aria-pressed={recentOpen}
            data-testid="recent-runs-toggle"
            onClick={() => toggleRecentRunsPanel()}
          >
            <RecentRunsIcon className="size-4" />
          </Button>
        </TooltipTrigger>
        <TooltipContent side="right">Recent runs</TooltipContent>
      </Tooltip>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div
          className={`mx-auto flex w-full max-w-2xl flex-col gap-6 px-4 py-6 md:min-h-full md:justify-center ${
            listEmpty ? `min-h-full justify-center` : ``
          } ${TAB_BAR_CLEARANCE}`}
        >
          {steerEnabled ? (
            // Keyed by team: the composer's repo pick and seed latches are
            // one-shot per mount, so a /t/a/agent → /t/b/agent navigation
            // remounts it instead of carrying team A's repo into team B.
            <ComposerPane
              key={team.id}
              teamId={team.id}
              remote={remote}
              users={teamUsers}
              seed={seed}
              onSeedConsumed={onSeedConsumed}
              onActionChange={onActionChange}
            />
          ) : (
            <p className="text-center text-sm text-muted-foreground">
              Live steering is unavailable on this instance.
            </p>
          )}
          {/* EXP-923: PHONE ONLY. On md+ what is running is a sidebar
              section, so the page is the composer alone; the phone keeps its
              Running list under the composer, inside the page's ONE scroller
              so it never traps a nested one. */}
          <SessionsList
            teamId={team.id}
            currentUserId={currentUserId}
            activeSessionId={null}
            origin={{ kind: `agent` }}
            scroll={false}
            // EXP-862: the Agent page ALWAYS draws the Running band, empty or
            // not.
            showWhenEmpty
            // The page's own container already reserves the tab bar's
            // clearance — the list must not add a second one inside it.
            className="p-0 max-md:pb-0 md:hidden"
          />
        </div>
      </div>
    </div>
  )
}

/** Mounted only with steering on: the hook wires the synced collections and
 * the options cluster, so it never runs for a member of a relay-less
 * instance. */
function ComposerPane({
  teamId,
  remote,
  users,
  seed,
  onSeedConsumed,
  onActionChange,
}: {
  teamId: string
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
  return <LaunchComposer model={model} users={users} />
}
