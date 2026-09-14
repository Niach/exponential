import type { CodingSession } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { cn } from "@/lib/utils"
import { useKillSession } from "@/hooks/use-kill-session"
import { useResumeRun } from "@/hooks/use-resume-run"
import { useSessionDevice } from "@/hooks/use-session-device"
import { Pill } from "@/components/ui/pill"
import { SessionMergeButton } from "@/components/session-merge-button"
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
export const MERGE_PR_LABEL = `Merge PR`

/** The primary PAINT on the glass capsule — the same accent fill `Pill`'s
 * `primary` flag draws, so Merge reads as the one call to action in its row
 * whatever the surface. */
export const MERGE_PILL_CLASS = `border-transparent bg-primary text-primary-foreground hover:bg-primary/90 hover:text-primary-foreground`

/** EXP-818: the ONE Stop — a small red-tinted glass pill, identical on the
 * machine that hosts the run and on one that only watches it (the IDE's
 * `stop_session_pill`). Presentational: the caller owns the confirm
 * (`useKillSession`) and renders its dialog. */
export function StopRunPill({
  onStop,
  className,
}: {
  onStop: () => void
  className?: string
}) {
  return (
    <Pill
      size="sm"
      mode="action"
      className={cn(`shrink-0 text-destructive`, className)}
      onClick={onStop}
      aria-label="Stop the agent and end the session"
      title="Stop the agent and end the session"
      data-testid="run-stop-pill"
    >
      <CodingStopIcon className="size-3" />
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
  className,
}: {
  session: CodingSession
  className?: string
}) {
  const { resuming, resume } = useResumeRun(session)
  return (
    <Pill
      size="sm"
      mode="action"
      className={cn(`shrink-0`, className)}
      disabled={resuming}
      onClick={() => void resume()}
      aria-label="Resume this run"
      title="Resume this run"
      data-testid="run-resume-pill"
    >
      {resuming ? (
        <UiLoadingIcon className="size-3 animate-spin" />
      ) : (
        <RunResumeIcon className="size-3" />
      )}
      {RESUME_LABEL}
    </Pill>
  )
}

/** Merge, the one look everywhere: the primary glass capsule with the merge
 * glyph and the two-click confirm `SessionMergeButton` already carries. It
 * self-hides unless the target's PR is open. */
export function MergePrPill({
  className,
  steerEnabled,
  ...target
}: SessionMergeTargetProps & {
  className?: string
  steerEnabled: boolean
}) {
  return (
    <SessionMergeButton
      {...target}
      variant="glass"
      size="sm"
      label={MERGE_PR_LABEL}
      className={cn(MERGE_PILL_CLASS, className)}
      steerEnabled={steerEnabled}
    />
  )
}
