import type { CSSProperties } from "react"
import { ExponentialLogo } from "@exp/ui"
import { HOME_GEOMETRY } from "@exp/domain-contract/home-greeting"
import { HOME_TEST_IDS } from "@exp/domain-contract/home-greeting"
import { GettingStartedSection } from "@/components/getting-started/getting-started-section"
import { LaunchComposer } from "@/components/launch-composer"
import { useLaunchComposer, type LaunchComposerHome } from "@/hooks/use-launch-composer"
import { useEnsureAssistant } from "@/hooks/use-ensure-assistant"
import { useSession } from "@/hooks/use-session"
import { useTeamBoardsWithReady, useTeamUsers } from "@/hooks/use-team-data"
import type { Team, User } from "@/db/schema"
import type { SteerDevice } from "@/lib/steer-devices"
import type { LaunchSeed } from "@/lib/launch-seed"

// EXP-1253 / #EXP-2027: THE home column (composer-home.json), shared by the
// team-less `/home` (web md+) and the phones' `/t/$teamSlug/agent`: the
// DAWN, the agent row + the composer (or the relay-less line), then, while
// the team has no board, Getting started. Nothing else.

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
  home = null,
}: {
  /** The page's team (Home: the default team, keying the draft). */
  team: Team
  isMember: boolean
  steerEnabled: boolean
  devices: SteerDevice[] | null
  /** The team roster for `@` (Home reads the effective team's itself). */
  users: User[]
  seed: LaunchSeed | null
  onSeedConsumed: () => void
  /** #EXP-2027: the team-less Home (every member team). */
  home?: LaunchComposerHome | null
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
          home={home}
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
export function HomeDawn() {
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
export function ComposerPane({
  teamId,
  devices,
  users,
  seed,
  onSeedConsumed,
  home = null,
}: {
  teamId: string
  devices: SteerDevice[] | null
  users: User[]
  seed: LaunchSeed | null
  onSeedConsumed: () => void
  home?: LaunchComposerHome | null
}) {
  const { data: authSession } = useSession()
  // S-3: the viewer's Assistant exists before the agent row lists it.
  // #EXP-2027: Home has no current team (an account-level ask).
  useEnsureAssistant(home ? null : teamId)
  const model = useLaunchComposer({
    teamId,
    devices,
    seed,
    onSeedConsumed,
    // S-3 (composer-home.json `agentRow`): the New-chat composer picks its
    // recipient, the viewer's Assistant preselected.
    agentRow: true,
    me: authSession?.user?.id ?? null,
    home,
  })
  // #EXP-2027: `@` lists the roster of the team the send lands in.
  const { users: effectiveUsers } = useTeamUsers(home ? model.teamId : undefined)
  return <LaunchComposer model={model} users={home ? effectiveUsers : users} variant="home" />
}
