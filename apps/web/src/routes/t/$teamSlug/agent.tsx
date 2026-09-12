import { useEffect, useMemo, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { useSteerConfig } from "@/components/agent-session"
import { SessionsList } from "@/components/agent-shell"
import { LaunchComposer } from "@/components/launch-composer"
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
  validateSearch: (
    search: Record<string, unknown>
  ): AgentSearch & { from?: string } => ({
    issues: str(search.issues),
    action: str(search.action),
    pr: str(search.pr),
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
    [search.issues, search.action, search.pr, search.device, search.text, search.icon]
  )
  const [seed, setSeed] = useState<LaunchSeed | null>(null)
  useEffect(() => {
    if (!urlSeed) return
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

  if (!team || !currentUserId) {
    return <div className="p-6 text-sm text-muted-foreground">Loading…</div>
  }

  // EXP-851: ONE column on every breakpoint, ONE scroller — the composer on
  // top, Running/Past under it. The side column is gone: an open run's list
  // lives in the sidebar's list nav now, so the page never nests a second
  // list beside itself (and never a second vertical scrollbar either).
  return (
    <div className="flex h-full min-h-0 flex-col" data-testid="agent-page">
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div
          className={`mx-auto flex w-full max-w-2xl flex-col gap-6 px-4 py-6 ${TAB_BAR_CLEARANCE}`}
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
              onSeedConsumed={() => setSeed(null)}
            />
          ) : (
            <p className="text-center text-sm text-muted-foreground">
              Live steering is unavailable on this instance.
            </p>
          )}
          {/* The same list the sidebar's Agent nav renders — here it is part
              of the page's ONE scroller, so it never traps a nested one. */}
          <SessionsList
            teamId={team.id}
            currentUserId={currentUserId}
            activeSessionId={null}
            origin={{ kind: `agent` }}
            scroll={false}
            // The page's own container already reserves the tab bar's
            // clearance — the list must not add a second one inside it.
            className="p-0 max-md:pb-0"
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
}: {
  teamId: string
  remote: RemoteStart
  users: User[]
  seed: LaunchSeed | null
  onSeedConsumed: () => void
}) {
  const model = useLaunchComposer({ teamId, remote, seed, onSeedConsumed })
  return <LaunchComposer model={model} users={users} />
}
