import {
  useEffect,
  useMemo,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Link } from "@tanstack/react-router"
import { ChevronRight, GitBranch, GitPullRequest } from "lucide-react"
import { conceptIcon, FabButton, Pill, GlassRow, LiveDot } from "@exp/ui"
import type { CodingSession, Issue, Board } from "@/db/schema"
import { useNow } from "@/hooks/use-now"
import { blockedBadgeLabel } from "@/lib/agent-usage"
import { issueCollection } from "@/lib/collections"
import { stackChain } from "@/lib/pr-stack"
import { trpc } from "@/lib/trpc-client"
import { cn } from "@/lib/utils"
import { useOpenComposer } from "@/hooks/use-open-composer"
import {
  useCodingReadiness,
  useIsTeamMember,
} from "@/hooks/use-coding-readiness"
import { READINESS_COPY } from "@/lib/coding-readiness"
import {
  CodingReadinessOverlay,
  ReadinessCaption,
  ReadinessStartPill,
} from "@/components/coding-readiness-checklist"

// The phone bar's start button — the same glyph the Actions surfaces run with.
const ActionRunIcon = conceptIcon(`action-run`)
// EXP-897: the PR stack's glyph — a concept, never a raw lucide import.
const StackIcon = conceptIcon(`pr-stack`)

// EXP-568: the floating mobile bar's 52px circle is `FabButton` (EXP-962);
// issue-detail-mobile-bar.tsx owns the bar itself, the coding circle's gating
// lives here.

// EXP-616: the coding / PR rows are glass CARDS now, not full-bleed
// `border-t` divider rows. Both exported pieces mount as independent siblings
// of the issue-detail main column (issue-detail-view.tsx owns no wrapper), so
// each one carries its own stack + gutter. EXP-698 r4 aligns that gutter with
// the properties band's (`max-w-4xl px-4 pt-3`) — the coding card sits
// directly under the band on both viewports, so the two must share an edge.
function CodingRowStack({ children }: { children: ReactNode }) {
  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-2 px-4 pt-3 pb-2">
      {children}
    </div>
  )
}

// The coding affordances of the issue detail (EXP-106): a compact "coding now"
// / remote-start control that NAVIGATES to the run's session page (EXP-740 —
// it never mounts the live viewer itself), plus a PR / pushed-branch row that
// links to the review-detail route. Membership + relay availability gate
// them (the same signals the server enforces); EXP-1121: a missing repository
// or device no longer hides Start coding — it turns the capsule into the
// readiness checklist's entry point.
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
  return <LiveDot tone="live" ping />
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
 * moved into the properties card beside Start coding, and the missing step
 * is the `start` variant's own caption (EXP-1121) — so an idle issue
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
// "Start coding" button — EXP-1121 moved it beside the readiness inputs.
export { useIsTeamMember }

/** The "coding now" / remote-start control — main-column row or phone circle. */
export function IssueCodingControl({
  issue,
  board,
  teamId,
  currentUserId,
  variant,
  tone = `primary`,
}: {
  issue: Issue
  board: Board
  teamId: string
  currentUserId: string
  variant: CodingControlVariant
  tone?: CodingStartTone
}) {
  // EXP-877: the tray's coding slot says nothing about a run any more. A
  // live run of mine is a sidebar row (EXP-923, `team/sidebar-running.tsx`)
  // and a teammate's is their own business (EXP-312), so `start` always
  // offers Start coding. EXP-893: the phone's `fab` is the same — a play
  // circle, never a Watch glyph. `row` draws nothing any more (EXP-818).
  if (variant === `row`) return null
  return (
    <ReadinessStartControl
      issue={issue}
      board={board}
      teamId={teamId}
      currentUserId={currentUserId}
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

// ── Start coding + the readiness checklist (EXP-1121) ─────────────────────

// Start coding ALWAYS renders for a member on an instance with remote start:
// the primary capsule once the issue can start right now, else the dashed
// amber one captioned with the FIRST missing step (GitHub, the board's
// repository, a device online). Either opens the "Ready to code?" checklist
// (`coding-readiness-checklist.tsx`) until ready; ready, it navigates to the
// composer with this issue as its subject (EXP-825), as it always did.
function ReadinessStartControl({
  issue,
  board,
  teamId,
  currentUserId,
  variant,
  tone,
}: {
  issue: Issue
  board: Board
  teamId: string
  currentUserId: string
  variant: Exclude<CodingControlVariant, `row`>
  tone: CodingStartTone
}) {
  const state = useCodingReadiness({
    teamId,
    boardIds: [board.id],
    currentUserId,
  })
  const { readiness } = state
  const openComposer = useOpenComposer()
  const [open, setOpen] = useState(false)

  // Non-member, or no remote start on this instance: nothing to offer.
  if (!readiness.visible) return null

  const start = () => openComposer({ issueIds: [issue.id] })
  const press = () => {
    if (readiness.loading) return
    if (readiness.ready) start()
    else setOpen((value) => !value)
  }
  const notReady = !readiness.loading && !readiness.ready

  if (variant === `fab`) {
    // EXP-1121: the play circle ALWAYS stays on a phone — dashed with an
    // amber badge dot while a step is missing; a tap opens the checklist
    // as a bottom sheet.
    return (
      <CodingReadinessOverlay
        state={state}
        open={open}
        onOpenChange={setOpen}
        onStart={start}
        surface="sheet"
      >
        <FabButton
          emphasis="primary"
          aria-label={READINESS_COPY.start}
          aria-disabled={readiness.loading || undefined}
          data-readiness={
            readiness.loading ? `loading` : readiness.ready ? `ready` : `missing`
          }
          className={cn(
            `relative`,
            notReady && `border-dashed border-amber-400/60 text-foreground/70`
          )}
          onClick={press}
        >
          <ActionRunIcon className="size-5" />
          {notReady && (
            <span
              aria-hidden
              data-testid="coding-readiness-badge"
              className="absolute top-2.5 right-2.5 size-2 rounded-full bg-amber-400 ring-2 ring-background"
            />
          )}
        </FabButton>
      </CodingReadinessOverlay>
    )
  }

  // EXP-616: the start affordance is a capsule INSIDE the issue's properties
  // card (desktop parity with the IDE) — no standalone row of its own.
  return (
    <CodingReadinessOverlay
      state={state}
      open={open}
      onOpenChange={setOpen}
      onStart={start}
      surface="popover"
    >
      {readiness.caption && (
        <ReadinessCaption caption={readiness.caption} onClick={press} />
      )}
      <ReadinessStartPill
        readiness={readiness}
        tone={tone}
        onClick={press}
        testId="issue-start-coding"
      />
    </CodingReadinessOverlay>
  )
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
        <StackChainLine issue={issue} teamSlug={teamSlug} />
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

/** EXP-897 §6: the issue's PR STACK, bottom-up, under its PR row. One quiet
 *  line of `#IDENT · <state>` — this issue in bold, every other member a link
 *  to its own review page. Absent when the pull request is in no stack. */
function StackChainLine({
  issue,
  teamSlug,
}: {
  issue: Issue
  teamSlug: string
}) {
  const { data: issueRows } = useLiveQuery(
    (query) =>
      query
        .from({ i: issueCollection })
        .where(({ i }) => eq(i.teamId, issue.teamId)),
    [issue.teamId]
  )
  const chain = useMemo(
    () => stackChain(issue, (issueRows ?? []) as Issue[]),
    [issue, issueRows]
  )
  if (chain.length < 2) return null
  return (
    <div
      className="flex flex-wrap items-center gap-x-2 gap-y-1 px-1 py-1 text-xs text-muted-foreground"
      data-testid="pr-stack-chain"
    >
      <StackIcon className="size-3 shrink-0" />
      {chain.map((member, index) => (
        <span key={member.id} className="flex items-center gap-1">
          {index > 0 && <span aria-hidden>→</span>}
          {member.id === issue.id ? (
            <span className="font-mono font-semibold text-foreground">
              {`#${member.identifier}`}
            </span>
          ) : (
            <Link
              to="/t/$teamSlug/reviews/$issueIdentifier"
              params={{ teamSlug, issueIdentifier: member.identifier }}
              className="font-mono underline-offset-2 hover:underline"
            >
              {`#${member.identifier}`}
            </Link>
          )}
          <PrStateBadge state={member.prState} />
        </span>
      ))}
    </div>
  )
}
