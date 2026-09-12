import {
  useEffect,
  useMemo,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { Link } from "@tanstack/react-router"
import {
  ChevronRight,
  GitBranch,
  GitPullRequest,
  MonitorUp,
} from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import type { CodingSession, Issue, Board, User } from "@/db/schema"
import { isCodingSessionStale } from "@exp/db-schema/domain"
import { useNow } from "@/hooks/use-now"
import { blockedBadgeLabel } from "@/lib/agent-usage"
import { useSessionDevice } from "@/hooks/use-session-device"
import { sessionIsPaused } from "@/lib/session-device"
import {
  codingSessionCollection,
  teamMemberCollection,
} from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { displayUserName } from "@/lib/user-display"
import { cn } from "@/lib/utils"
import { Pill } from "@/components/ui/pill"
import { GlassRow } from "@/components/ui/glass-rows"
import { useSteerConfig } from "@/components/agent-session"
import { useOpenSession } from "@/hooks/use-open-session"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useOpenComposer } from "@/hooks/use-open-composer"

// EXP-317: the "no desktop online" hint draws the same glyph here and in
// the native apps (`ui-device-offline`).
const UiDeviceOfflineIcon = conceptIcon(`ui-device-offline`)
// The phone bar's start button — the same glyph the Actions surfaces run with.
const ActionRunIcon = conceptIcon(`action-run`)
// EXP-698 r4: the Watch pill draws the concept the natives draw on their own
// Watch pill, never a raw lucide glyph.
const WatchIcon = conceptIcon(`nav-devices`)

// EXP-568: the floating mobile bar's 52px circles (issue-detail-mobile-bar.tsx
// owns the bar itself; the coding circle's gating lives here).
const FAB_CIRCLE_CLASS = `pointer-events-auto flex size-[52px] shrink-0 items-center justify-center rounded-full border border-glass-stroke-card bg-popover/85 shadow-lg shadow-black/40 backdrop-blur-xl`

// EXP-616: the coding / PR rows are glass CARDS now, not full-bleed
// `border-t` divider rows. Both exported pieces mount as independent siblings
// of the issue-detail main column (issue-detail-view.tsx owns no wrapper), so
// each one carries its own stack + gutter. EXP-698 r4 aligns that gutter with
// the properties band's (`max-w-3xl px-4 pt-3`) — the coding card sits
// directly under the band on both viewports, so the two must share an edge.
function CodingRowStack({ children }: { children: ReactNode }) {
  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-2 px-4 pt-3 pb-2">
      {children}
    </div>
  )
}

// The coding affordances of the issue detail (EXP-106): a compact "coding now"
// / remote-start control that NAVIGATES to the run's session page (EXP-740 —
// it never mounts the live viewer itself), plus a PR / pushed-branch row that
// links to the review-detail route. Repo presence + membership + relay availability gate them (the same
// signals the server enforces); everything degrades to nothing when absent.
// EXP-184 split them: IssueCodingControl renders as the full-width main-column
// row (variant='row', both viewports since EXP-568) or as the phone bottom
// bar's circle (variant='fab'); IssuePrRow always stays a main-column row.
// EXP-616 added variant='start': the bare "Start coding" capsule the issue
// detail hangs off its properties card, which the 'row' variant therefore no
// longer draws.

/** PR-state pill — open emerald / merged purple / closed rose / draft plain. */
export function PrStateBadge({ state }: { state: string | null | undefined }) {
  if (!state) return null
  if (state === `draft`) return <Pill>Draft</Pill>
  const styles: Record<string, string> = {
    open: `text-emerald-400`,
    merged: `text-purple-400`,
    closed: `text-rose-400`,
  }
  const cls = styles[state]
  if (!cls) return null
  return <Pill className={cn(`capitalize`, cls)}>{state}</Pill>
}

function RunningPing() {
  return (
    <span className="relative flex size-2">
      <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-60" />
      <span className="relative inline-flex size-2 rounded-full bg-emerald-500" />
    </span>
  )
}

// The display-state derivation lives in a plain lib module so it can be
// unit-tested without dragging the component graph in (EXP-531); re-exported
// here for the existing importers.
import {
  sessionDisplayState,
  sessionRowIsWorking,
  type SessionDisplayState,
} from "@/lib/coding-session-display"

export { sessionDisplayState, type SessionDisplayState }

/** Static counterpart of RunningPing for the parked states (EXP-194/EXP-214):
 * review green (matches the in_review issue status), done blue (matches the
 * done issue status), needs-input amber. */
function StateDot({ className }: { className: string }) {
  return <span className={cn(`inline-flex size-2 rounded-full`, className)} />
}

// EXP-698 r5: the coding-now badge is the readonly `sm` pill on every client —
// a tone dot, the tone as the text colour, and the tone at 40% as the stroke
// (IDE `.border_color(tone.opacity(0.4))`, iOS/Android `GlassPill(tint:)`).
// The tones are the Tailwind palette vars so the inline mix and the dot draw
// the SAME colour the class-based rows draw.
const SESSION_STATE_BADGE: Record<
  Exclude<SessionDisplayState, `running`>,
  { label: string; tone: string }
> = {
  needs_input: { label: `Needs input`, tone: `var(--color-amber-400)` },
  review: { label: `Ready for review`, tone: `var(--color-emerald-400)` },
  done: { label: `Done`, tone: `var(--color-sky-400)` },
}

const RUNNING_TONE = `var(--color-emerald-400)`
const PAUSED_TONE = `var(--muted-foreground)`
// EXP-804: the usage wall reuses the "Needs input" amber — both mean the run
// is alive but cannot move until something outside it changes.
const BLOCKED_TONE = `var(--color-amber-400)`

/** The phone bar's badge dot — the pulsing ping while the agent WORKS
 * (EXP-848: `working`, never the bare `running` state), the badge's own tone
 * once it parks, so the circle and the pill never disagree about a state's
 * colour (iOS `sessionDot`, Android's mirror). */
function SessionStateDot({
  state,
  working = false,
}: {
  state: SessionDisplayState
  working?: boolean
}) {
  if (state === `running`) {
    return working ? (
      <RunningPing />
    ) : (
      <span
        className="inline-flex size-2 rounded-full"
        style={{ backgroundColor: RUNNING_TONE }}
      />
    )
  }
  return (
    <span
      className="inline-flex size-2 rounded-full"
      style={{ backgroundColor: SESSION_STATE_BADGE[state].tone }}
    />
  )
}

/** Text in the tone, stroke in the tone at 40% — the readonly pill's tint. */
function toneStyle(tone: string): CSSProperties {
  return {
    color: tone,
    borderColor: `color-mix(in srgb, ${tone} 40%, transparent)`,
  }
}

/** EXP-804: the agent's usage wall, rendered BESIDE the state badge and never
 * instead of it. A walled run is still `running` — live, steerable, killable
 * — so the wall composes with the state exactly the way `needs_input` does;
 * collapsing the two would hide either that the run is alive or that it
 * cannot make a call. Amber, the same attention tone "Needs input" uses.
 *
 * Renders nothing when the run is not blocked, so every call site is a bare
 * mount with no surrounding condition. Shared by the issue detail row, the
 * Agents page, the session header and the Devices page.
 */
export function SessionBlockedBadge({
  session,
}: {
  session: Pick<CodingSession, `blocked`>
}) {
  const now = useNow(30_000)
  const label = blockedBadgeLabel(session.blocked, now)
  if (!label) return null
  const tone = BLOCKED_TONE
  return (
    <Pill size="sm" style={toneStyle(tone)} dot={tone}>
      {label}
    </Pill>
  )
}

/** Live-session badge — "Coding now" / "Needs input" / "Ready for review" /
 * "Done". Shared by the issue detail row and the Agents page. */
export function SessionStatusBadge({
  session,
  prState,
  count = 1,
  paused = false,
}: {
  session: Pick<CodingSession, `status` | `needsInput` | `agentBusy`>
  prState: string | null | undefined
  count?: number
  /** EXP-550: the host machine is offline — the agent is parked, not gone.
   * Renders a grey "Paused" instead of the live/needs-input badge. */
  paused?: boolean
}) {
  const state = sessionDisplayState(session, prState)
  if (paused) {
    return (
      <Pill
        size="sm"
        style={toneStyle(PAUSED_TONE)}
        dot={`color-mix(in srgb, ${PAUSED_TONE} 40%, transparent)`}
      >
        Paused
        {count > 1 ? ` (·${count})` : ``}
      </Pill>
    )
  }
  if (state === `running`) {
    // The pulsing ping IS the dot while the agent WORKS (EXP-848) — it rides
    // the `leading` slot so the ripple has room the 6px disc would clip. An
    // idle-between-turns run keeps the badge and drops the ripple.
    const working = sessionRowIsWorking(session, prState)
    return (
      <Pill
        size="sm"
        style={toneStyle(RUNNING_TONE)}
        leading={working ? <RunningPing /> : undefined}
        dot={working ? undefined : RUNNING_TONE}
      >
        Coding now
        {count > 1 ? ` (·${count})` : ``}
      </Pill>
    )
  }
  const style = SESSION_STATE_BADGE[state]
  return (
    <Pill size="sm" style={toneStyle(style.tone)} dot={style.tone}>
      {style.label}
      {count > 1 ? ` (·${count})` : ``}
    </Pill>
  )
}

/** `row` = the main-column "coding now" card and NOTHING else (EXP-760: Merge
 * moved into the properties card beside Start coding, and the "no desktop
 * online" hint became the `start` variant's own caption — so an idle issue
 * draws no main-column card at all), `fab` = the phone bar's circle, `start` =
 * the "Start coding" capsule the issue detail's properties card hosts
 * (EXP-616, desktop parity with the IDE). The two mounts never draw the same
 * affordance twice. */
export type CodingControlVariant = `row` | `fab` | `start`

/** EXP-760: how loud the `start` capsule is. Start coding is the one call to
 * action on an idle issue (`primary`), but it steps down to the glass form the
 * moment an open PR puts a white **Merge** button beside it — two accent pills
 * in one slot say nothing about which one to press. */
export type CodingStartTone = `primary` | `glass`

// Membership gate shared by both exported pieces and the bulk bar's
// "Start coding" button (the server enforces it regardless; this only decides
// what renders).
export function useIsTeamMember(teamId: string, currentUserId: string) {
  const { data: memberRows } = useLiveQuery(
    (query) =>
      query
        .from({ m: teamMemberCollection })
        .where(({ m }) =>
          and(eq(m.teamId, teamId), eq(m.userId, currentUserId))
        ),
    [teamId, currentUserId]
  )
  return (memberRows?.length ?? 0) > 0
}

/** The "coding now" / remote-start control — main-column row or phone circle. */
export function IssueCodingControl({
  issue,
  board,
  teamId,
  currentUserId,
  users,
  variant,
  tone = `primary`,
}: {
  issue: Issue
  board: Board
  teamId: string
  currentUserId: string
  users: User[]
  variant: CodingControlVariant
  tone?: CodingStartTone
}) {
  const config = useSteerConfig()
  const isMember = useIsTeamMember(teamId, currentUserId)

  return (
    <AgentRow
      issue={issue}
      board={board}
      teamId={teamId}
      currentUserId={currentUserId}
      users={users}
      isMember={isMember}
      steerEnabled={config?.enabled ?? null}
      variant={variant}
      tone={tone}
    />
  )
}

/** The PR / pushed-branch main-column row. */
export function IssuePrRow({
  issue,
  board,
  teamId,
  teamSlug,
  currentUserId,
}: {
  issue: Issue
  board: Board
  teamId: string
  teamSlug: string
  currentUserId: string
}) {
  const isMember = useIsTeamMember(teamId, currentUserId)

  return (
    <PrRow
      issue={issue}
      board={board}
      teamSlug={teamSlug}
      isMember={isMember}
    />
  )
}

// ── Running / remote-start row ────────────────────────────────────────────────

function AgentRow({
  issue,
  board,
  teamId,
  currentUserId,
  users,
  isMember,
  steerEnabled,
  variant,
  tone,
}: {
  issue: Issue
  board: Board
  teamId: string
  currentUserId: string
  users: User[]
  isMember: boolean
  /** null while steer.config is still loading. */
  steerEnabled: boolean | null
  variant: CodingControlVariant
  tone: CodingStartTone
}) {
  const openSession = useOpenSession()

  const { data: sessionRows } = useLiveQuery(
    (query) =>
      query
        .from({ s: codingSessionCollection })
        .where(({ s }) =>
          and(
            eq(s.issueId, issue.id),
            inArray(s.status, [`running`, `in_review`])
          )
        ),
    [issue.id]
  )
  // Staleness guard (EXP-153): heartbeat-dead rows render as absent.
  // Multi-window desktops can run several sessions on one issue; surface the
  // most recent (the badge counts them all).
  const now = useNow()
  const sessions = ((sessionRows ?? []) as CodingSession[]).filter(
    (s) => !isCodingSessionStale(s.updatedAt, now)
  )
  const latest = useMemo(() => {
    if (sessions.length === 0) return null
    return sessions.reduce((newest, row) =>
      new Date(row.startedAt) > new Date(newest.startedAt) ? row : newest
    )
  }, [sessions])
  // EXP-312: live sessions are owner-only — the Watch affordance targets the
  // caller's own most-recent session (teammates see just badge + byline).
  const ownLatest = useMemo(() => {
    const own = sessions.filter((s) => s.userId === currentUserId)
    if (own.length === 0) return null
    return own.reduce((newest, row) =>
      new Date(row.startedAt) > new Date(newest.startedAt) ? row : newest
    )
  }, [sessions, currentUserId])
  // EXP-549/550: the latest session's host machine per the synced devices
  // row — renamed label, and "Paused" while that machine is offline.
  const latestDevice = useSessionDevice(latest)

  if (latest) {
    const owner = users.find((u) => u.id === latest.userId)
    const paused = sessionIsPaused(
      sessionDisplayState(latest, issue.prState),
      latestDevice
    )

    // EXP-568 phone bar: one 52px circle, no words. Own live session → tap to
    // open its session page; someone else's → a static badge circle that says
    // "busy, not yours" (EXP-312 keeps live sessions owner-only).
    // EXP-698 r7: the circle NAMES what it opens — the Devices/monitor glyph —
    // and the state dot rides its top-trailing corner as a badge, clear of the
    // glyph's own bounds (iOS/Android `sessionGlyph`). A bare dot in a glass
    // circle said nothing about where the tap went.
    if (variant === `fab`) {
      const dot = paused ? (
        <StateDot className="bg-muted-foreground/40" />
      ) : (
        <SessionStateDot
          state={sessionDisplayState(latest, issue.prState)}
          working={sessionRowIsWorking(latest, issue.prState)}
        />
      )
      const glyph = (
        <span className="relative flex">
          <WatchIcon className="size-5" />
          <span className="absolute -right-1.5 -top-1">{dot}</span>
        </span>
      )
      if (ownLatest && steerEnabled) {
        return (
          <button
            type="button"
            aria-label="Open coding session"
            onClick={() => openSession(ownLatest)}
            className={cn(FAB_CIRCLE_CLASS, `text-foreground`)}
          >
            {glyph}
          </button>
        )
      }
      return (
        <div
          aria-label="Coding session running"
          className={cn(FAB_CIRCLE_CLASS, `text-muted-foreground`)}
        >
          {glyph}
        </div>
      )
    }

    // EXP-818: the tray's coding SLOT (IDE `coding_now_slot` twin) — the
    // caller's own run as the primary Watch pill straight into the run's
    // screen; a teammate's as a muted `● Coding now · name` caption. The
    // "coding now" card under the tray is gone: the tray already holds every
    // action, and a second card said the same thing again.
    if (variant === `start`) {
      if (ownLatest && steerEnabled) {
        return (
          <Pill
            size="sm"
            mode="action"
            primary
            onClick={() => openSession(ownLatest)}
          >
            <WatchIcon />
            Watch
          </Pill>
        )
      }
      const state = paused ? null : sessionDisplayState(latest, issue.prState)
      const verb = paused
        ? `Paused`
        : state === `needs_input`
          ? `Needs input`
          : state === `review`
            ? `Ready for review`
            : state === `done`
              ? `Done`
              : `Coding now`
      return (
        <span
          className="flex min-w-0 shrink-0 items-center gap-1.5 text-xs text-muted-foreground"
          title={paused ? `${latestDevice.label ?? `The device`} is offline` : undefined}
        >
          {paused ? (
            <StateDot className="bg-muted-foreground/40" />
          ) : (
            <SessionStateDot
              state={sessionDisplayState(latest, issue.prState)}
              working={sessionRowIsWorking(latest, issue.prState)}
            />
          )}
          <span className="truncate">
            {verb}
            {ownLatest ? `` : ` · ${displayUserName(owner, latest.userId)}`}
          </span>
        </span>
      )
    }
    // `row` draws nothing any more (EXP-818).
    return null
  }

  // Not running: only members can remote-start, and only on a repo-backed
  // board with the relay enabled. Gate the device wiring behind that —
  // RemoteStartRow (which owns useRemoteStart over the synced devices shape)
  // mounts ONLY here, so a non-member / steer-off / repo-less /
  // already-running issue view never wires it up.
  if (!isMember || !steerEnabled || !board.repositoryId) {
    // Nothing left to draw: the open PR's Merge button lives in the properties
    // card now (EXP-760, `issue-detail-view.tsx`), where it renders whether or
    // not remote start is available on this instance.
    return null
  }
  return (
    <RemoteStartRow
      issue={issue}
      teamId={teamId}
      currentUserId={currentUserId}
      variant={variant}
      tone={tone}
    />
  )
}

// The remote-start affordance — split out so its device wiring only
// runs when the start row can actually render (AgentRow gates the mount).
function RemoteStartRow({
  issue,
  teamId,
  currentUserId,
  variant,
  tone,
}: {
  issue: Issue
  teamId: string
  currentUserId: string
  variant: CodingControlVariant
  tone: CodingStartTone
}) {
  // EXP-825: the devices ride only the "No desktop online" caption now —
  // the click itself is a navigation to the Agent page composer with this
  // issue as the subject chip (the launch dialog is gone).
  const remote = useRemoteStart({ currentUserId, teamId })
  const openComposer = useOpenComposer()

  // Presence lookup still in flight — keep the section quiet.
  if (remote.devices === null) return null
  if (remote.devices.length === 0) {
    // Nothing to start on: the phone bar simply drops the circle rather than
    // spending one of its three slots on an explanation. EXP-760: the
    // explanation is the START slot's own caption now — it used to be a
    // main-column row, which put an empty grey card under every issue of a
    // team whose desktops happen to be closed.
    if (variant !== `start`) return null
    return (
      <span className="flex min-w-0 items-center gap-1.5 truncate text-xs text-muted-foreground">
        <UiDeviceOfflineIcon className="size-3.5 shrink-0" />
        <span className="truncate">No desktop online</span>
      </span>
    )
  }

  const start = () => openComposer({ issueIds: [issue.id] })

  if (variant === `fab`) {
    return (
      <button
        type="button"
        aria-label="Start coding"
        onClick={start}
        className={cn(FAB_CIRCLE_CLASS, `text-foreground`)}
      >
        <ActionRunIcon className="size-5" />
      </button>
    )
  }

  // EXP-616: the start affordance is a capsule INSIDE the issue's properties
  // card (desktop parity with the IDE) — no standalone row of its own.
  if (variant === `start`) {
    return (
      <div className="flex min-w-0 items-center gap-2">
        <Pill mode="action" primary={tone === `primary`} onClick={start}>
          <MonitorUp />
          Start coding
        </Pill>
      </div>
    )
  }

  // `row` on an idle issue draws nothing at all: the capsule above carries
  // the start, and Merge sits beside it in the properties card (EXP-760).
  return null
}

// ── PR / pushed-branch row ────────────────────────────────────────────────────

function PrRow({
  issue,
  board,
  teamSlug,
  isMember,
}: {
  issue: Issue
  board: Board
  teamSlug: string
  isMember: boolean
}) {
  const hasPr = issue.prNumber != null
  // Tier-3 probe: a pushed branch with no PR yet, only while the issue is in a
  // coding-ish state (in_progress/in_review). One lookup per issue-id mount.
  const canProbe =
    !hasPr &&
    isMember &&
    Boolean(board.repositoryId) &&
    (issue.status === `in_progress` || issue.status === `in_review`)
  const [branchFileCount, setBranchFileCount] = useState<number | null>(null)

  useEffect(() => {
    if (!canProbe) {
      setBranchFileCount(null)
      return
    }
    let cancelled = false
    trpc.repositories.branchDiff
      .query({ issueId: issue.id })
      .then((res) => {
        if (!cancelled) setBranchFileCount(res?.files.length ?? 0)
      })
      .catch(() => {
        if (!cancelled) setBranchFileCount(null)
      })
    return () => {
      cancelled = true
    }
  }, [canProbe, issue.id])

  if (hasPr) {
    return (
      <CodingRowStack>
        <GlassRow asChild interactive className="min-w-0 gap-2 text-sm">
          <Link
            to="/t/$teamSlug/reviews/$issueIdentifier"
            params={{ teamSlug, issueIdentifier: issue.identifier }}
          >
            <GitPullRequest className="size-4 shrink-0 text-muted-foreground" />
            <PrStateBadge state={issue.prState} />
            <span className="shrink-0 font-mono">PR #{issue.prNumber}</span>
            {issue.branch && (
              <span className="hidden truncate font-mono text-xs text-muted-foreground md:inline">
                {issue.branch}
              </span>
            )}
            <ChevronRight className="ml-auto size-4 shrink-0 text-muted-foreground" />
          </Link>
        </GlassRow>
      </CodingRowStack>
    )
  }

  if (canProbe && branchFileCount != null && branchFileCount > 0) {
    return (
      <CodingRowStack>
        <GlassRow asChild interactive className="min-w-0 gap-2 text-sm">
          <Link
            to="/t/$teamSlug/reviews/$issueIdentifier"
            params={{ teamSlug, issueIdentifier: issue.identifier }}
          >
            <GitBranch className="size-4 shrink-0 text-muted-foreground" />
            <span className="truncate">
              Branch <span className="font-mono">exp/{issue.identifier}</span>
              {` · no PR yet`}
            </span>
            <ChevronRight className="ml-auto size-4 shrink-0 text-muted-foreground" />
          </Link>
        </GlassRow>
      </CodingRowStack>
    )
  }

  return null
}
