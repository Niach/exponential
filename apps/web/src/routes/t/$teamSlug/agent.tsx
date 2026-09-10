import { useEffect, useMemo, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { useSteerConfig } from "@/components/agent-session"
import { AgentShell, SessionsList } from "@/components/agent-shell"
import { LaunchComposer } from "@/components/launch-composer"
import { useLaunchComposer } from "@/hooks/use-launch-composer"
import { useIsMobile } from "@/hooks/use-mobile"
import { useRemoteStart, type RemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { conceptIcon } from "@/lib/icons.generated"
import type { User } from "@/db/schema"
import {
  seedFromSearch,
  type AgentSearch,
  type LaunchSeed,
} from "@/lib/launch-seed"

const str = (value: unknown): string | undefined =>
  typeof value === `string` && value !== `` ? value : undefined

// EXP-818: the team's AGENT page — the sessions list on the left (Running,
// then Past) and, on the right, the composer. EXP-825 made that composer THE
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
//
// The seed is ONE-SHOT: consumed by the composer, then stripped with a
// replace navigation (the `actions.tsx` `?editAction=` pattern), so a
// back/refresh never re-preselects. Clicking a session opens
// `/t/$teamSlug/sessions/$sessionId`, which renders inside the same shell.

const UiBackIcon = conceptIcon(`ui-back`)
const ActionChatIcon = conceptIcon(`action-chat`)

export const Route = createFileRoute(`/t/$teamSlug/agent`)({
  validateSearch: (search: Record<string, unknown>): AgentSearch => ({
    issues: str(search.issues),
    action: str(search.action),
    pr: str(search.pr),
    device: str(search.device),
    text: str(search.text),
    icon: str(search.icon),
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
  const isMobile = useIsMobile()
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
      search: {},
      replace: true,
    })
  }, [urlSeed, navigate, teamSlug])

  if (!team || !currentUserId) {
    return <div className="p-6 text-sm text-muted-foreground">Loading…</div>
  }

  const prompt = (
    <div className="flex h-full min-h-0 flex-col" data-testid="agent-page">
      <ChatHeader title="Agent" />
      {/* EXP-772: an essentially empty pane — one centred composer with a
          subtle options line under it. */}
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex min-h-full w-full max-w-2xl flex-col gap-10 px-4 py-6">
          {steerEnabled ? (
            <ComposerPane
              teamId={team.id}
              remote={remote}
              users={teamUsers}
              seed={seed}
              onSeedConsumed={() => setSeed(null)}
            />
          ) : (
            <p className="my-auto text-center text-sm text-muted-foreground">
              Live steering is unavailable on this instance.
            </p>
          )}
        </div>
      </div>
      {/* The phone: the list under the composer — the page is both. */}
      {isMobile && (
        <SessionsList
          teamId={team.id}
          currentUserId={currentUserId}
          activeSessionId={null}
          className="shrink-0"
        />
      )}
    </div>
  )

  return (
    <AgentShell teamId={team.id} currentUserId={currentUserId} activeSessionId={null}>
      {prompt}
    </AgentShell>
  )
}

/** The prompt pane's own header — the AgentSessionView draws its own. */
function ChatHeader({ title }: { title: string }) {
  return (
    <div className="flex items-center gap-1 border-b border-border px-3 py-1.5">
      <UiBackIcon className="hidden" aria-hidden />
      <ActionChatIcon className="size-3.5 text-muted-foreground" />
      <span className="min-w-0 flex-1 truncate text-sm font-medium">{title}</span>
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
  return <LaunchComposer model={model} users={users} className="my-auto" />
}
