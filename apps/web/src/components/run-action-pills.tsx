import type { CodingSession } from "@/db/schema"
import { contract } from "@exp/domain-contract"
import { conceptIcon, Pill } from "@exp/ui"
import { cn } from "@/lib/utils"
import { useKillSession } from "@/hooks/use-kill-session"
import { useResumeRun } from "@/hooks/use-resume-run"
import { useSessionDevice } from "@/hooks/use-session-device"
import { SessionMergePill } from "@/components/session-merge-button"
import type { SessionMergeTargetProps } from "@/hooks/use-agents-data"

// EXP-877: the run's action pills, ONE look wherever a run is acted on — the
// issue tray's coding slot, the run header's right cluster, the phone's
// compact row. Byte-identical verbs with the IDE (`work_header.rs`): `Stop`,
// `Resume`, `Merge PR`.

const CodingStopIcon = conceptIcon(`coding-stop`)
const RunResumeIcon = conceptIcon(`run-resume`)
const UiLoadingIcon = conceptIcon(`ui-loading`)

export const STOP_LABEL = `Stop`
export const RESUME_LABEL = `Resume`
export const MERGE_PR_LABEL = contract.diffUi.mergePr

// EXP-926 / FEED-45: a run pill has exactly TWO placements, and one size rule
// each — a `sm` Stop sitting beside a 36px face toggle read as an afterthought
// next to it.
//
//   `tray`   — inside the issue's properties card, so it is a CHIP: the `sm`
//              Pill the status / priority / label pickers are (h-6, 12px).
//   `header` — the work header's right cluster, beside the face toggle
//              (`SEGMENTED_LIST`, h-9) and the 36px GitHub circle: the `md`
//              Pill stretched to the toggle's own height.
//
// Nothing else may hand these pills a height. EXP-1079: the work header's
// graph badge (`pr-graph-badge.tsx`) stands in the same cluster, so it reads
// the SAME recipe — exported for it, never restated.
export type RunPillPlacement = `tray` | `header`

export const PLACEMENT_SIZE: Record<RunPillPlacement, `sm` | `md`> = {
  tray: `sm`,
  header: `md`,
}
/** The face toggle's height — `SEGMENTED_LIST` is `h-9`. */
const HEADER_PILL_CLASS = `h-9`
/** `md` pills draw 16px glyphs, `sm` ones 12px. */
export const PLACEMENT_GLYPH: Record<RunPillPlacement, string> = {
  tray: `size-3`,
  header: `size-4`,
}

export function placementClass(placement: RunPillPlacement): string | undefined {
  return placement === `header` ? HEADER_PILL_CLASS : undefined
}

/** EXP-818: the ONE Stop — a small red-tinted glass pill, identical on the
 * machine that hosts the run and on one that only watches it (the IDE's
 * `stop_session_pill`). Presentational: the caller owns the confirm
 * (`useKillSession`) and renders its dialog. */
export function StopRunPill({
  onStop,
  placement = `tray`,
  className,
}: {
  onStop: () => void
  placement?: RunPillPlacement
  className?: string
}) {
  return (
    <Pill
      size={PLACEMENT_SIZE[placement]}
      mode="action"
      className={cn(
        `shrink-0 text-destructive`,
        placementClass(placement),
        className
      )}
      onClick={onStop}
      aria-label="Stop the agent and end the session"
      title="Stop the agent and end the session"
      data-testid="run-stop-pill"
    >
      <CodingStopIcon className={PLACEMENT_GLYPH[placement]} />
      {STOP_LABEL}
    </Pill>
  )
}

/** Stop for a run the caller merely holds the ROW of (the issue tray): the
 * kill confirm and its dialog come along. Renders nothing unless the row is
 * live, mine and not parked on an offline machine. */
export function SessionStopRunPill({
  session,
  currentUserId,
  className,
}: {
  session: CodingSession
  currentUserId: string
  className?: string
}) {
  const device = useSessionDevice(session)
  const { canKill, requestKill, dialog } = useKillSession(
    session,
    currentUserId,
    device.label,
    device.online === false
  )
  if (!canKill) return null
  return (
    <>
      <StopRunPill onStop={requestKill} className={className} />
      {dialog}
    </>
  )
}

/** EXP-773: Resume relaunches an ended run on the machine that still holds
 * its worktree. The caller gates on `useCanResumeOn`; the pill stays busy
 * until the new row syncs in and opens (`useResumeRun`). */
export function ResumeRunPill({
  session,
  placement = `tray`,
  className,
}: {
  session: CodingSession
  placement?: RunPillPlacement
  className?: string
}) {
  const { resuming, resume } = useResumeRun(session)
  return (
    <Pill
      size={PLACEMENT_SIZE[placement]}
      mode="action"
      className={cn(`shrink-0`, placementClass(placement), className)}
      disabled={resuming}
      onClick={() => void resume()}
      aria-label="Resume this run"
      title="Resume this run"
      data-testid="run-resume-pill"
    >
      {resuming ? (
        <UiLoadingIcon
          className={cn(PLACEMENT_GLYPH[placement], `animate-spin`)}
        />
      ) : (
        <RunResumeIcon className={PLACEMENT_GLYPH[placement]} />
      )}
      {RESUME_LABEL}
    </Pill>
  )
}

/** Merge, the one look everywhere: the primary `Pill` with the merge glyph and
 * the two-click confirm `SessionMergePill` already carries (EXP-895 folded the
 * hand-rolled accent class into `Pill`'s own `primary`). It self-hides unless
 * the target's PR is open. */
export function MergePrPill({
  className,
  steerEnabled,
  placement = `tray`,
  ...target
}: SessionMergeTargetProps & {
  className?: string
  steerEnabled: boolean
  placement?: RunPillPlacement
}) {
  return (
    <SessionMergePill
      {...target}
      // EXP-889/EXP-926: the placement decides the box — a chip among the
      // tray's chips, the toggle's own height in the work header.
      pillSize={PLACEMENT_SIZE[placement]}
      label={MERGE_PR_LABEL}
      className={cn(`shrink-0`, placementClass(placement), className)}
      steerEnabled={steerEnabled}
    />
  )
}
