import { useState } from "react"
import type { CodingSession } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { isLiveRunStatus } from "@/lib/past-runs"
import { SESSION_DOT_CLASS, type SessionDotTone } from "@/lib/session-dot"
import { cn } from "@/lib/utils"
import {
  CHANGES_FACE_LABEL,
  faceLabel,
  START_CODING_LABEL,
  switcherBadge,
  switcherMode,
  switcherTargets,
  type SwitcherTarget,
  type WorkFaceKind,
} from "@/lib/work-faces"
import type { PastRunRow } from "@/hooks/use-agents-data"
import { issueRunEntryLabel } from "@/components/issue-run-switcher"
import { MOBILE_WORK_CIRCLE_CLASS } from "@/components/mobile-work-bar"
import { DiffFaceLabel } from "@/components/team/work-face-toggle"
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

// EXP-893: the phone's FACE SWITCHER — the bottom-right circle of the Work
// screen, the desktop face toggle's touch twin. With exactly one other
// target it wears the destination's icon and switches on tap; with two or
// more it wears the faces glyph and opens a menu above itself (✕ while
// open). The menu lists the other faces, one row per own run of the issue
// once there are two or more (EXP-886's byline, a check on the run shown)
// and `Start coding` when the shown run ended for good. The badge dot says
// what waits behind it: the session's state off the Run face, a green dot
// for changes on it. Pure rules in `lib/work-faces.ts`; iOS
// `WorkFaceSwitcher`, Android `FaceSwitcherCircle` mirror it.

const IssueIcon = conceptIcon(`ui-issue`)
const RunIcon = conceptIcon(`nav-devices`)
const ChangesIcon = conceptIcon(`coding-diff`)
const StartIcon = conceptIcon(`action-run`)
const FacesIcon = conceptIcon(`work-faces`)
const CloseIcon = conceptIcon(`ui-close`)

const FACE_ICON: Record<WorkFaceKind, typeof IssueIcon> = {
  issue: IssueIcon,
  run: RunIcon,
  changes: ChangesIcon,
}

export interface MobileFaceSwitcherProps {
  /** The faces this subject has (`availableFaces`). */
  faces: readonly WorkFaceKind[]
  /** The face on show. */
  face: WorkFaceKind
  /** The issue's runs of mine (`useIssueRuns`), switcher order. */
  runs?: readonly PastRunRow[]
  /** The run the Run face shows (or would show). */
  viewedRunId?: string | null
  /** The shown run has a live diff (the badge on the Run face, the +/- in
   *  the Changes row). */
  diffStats?: { additions: number; deletions: number } | null
  /** Changes exist at all (a live diff OR an open PR): the Run face's badge. */
  hasChanges: boolean
  /** The shown session's state dot, for the badge off the Run face. */
  sessionTone?: SessionDotTone | null
  /** The shown run ended and cannot be resumed: the menu offers Start. */
  offerStart?: boolean
  onFace: (face: WorkFaceKind) => void
  onOpenRun?: (session: CodingSession) => void
  onStart?: () => void
}

function targetLabel(
  target: SwitcherTarget,
  multipleRuns: boolean
): string {
  if (target.kind === `startCoding`) return START_CODING_LABEL
  if (target.kind === `run`) return faceLabel(`run`)
  return faceLabel(target.face, multipleRuns)
}

function TargetIcon({
  target,
  className,
}: {
  target: SwitcherTarget
  className?: string
}) {
  const Icon =
    target.kind === `startCoding`
      ? StartIcon
      : target.kind === `run`
        ? RunIcon
        : FACE_ICON[target.face]
  return <Icon className={className} />
}

function BadgeDot({ tone }: { tone: SessionDotTone | `changes` }) {
  return (
    <span
      aria-hidden
      data-testid="mobile-face-switcher-badge"
      data-tone={tone}
      className={cn(
        `absolute -right-0.5 -top-0.5 size-2.5 rounded-full ring-2 ring-popover`,
        tone === `changes` ? SESSION_DOT_CLASS.running : SESSION_DOT_CLASS[tone]
      )}
    />
  )
}

export function MobileFaceSwitcher({
  faces,
  face,
  runs = [],
  viewedRunId = null,
  diffStats = null,
  hasChanges,
  sessionTone = null,
  offerStart = false,
  onFace,
  onOpenRun,
  onStart,
}: MobileFaceSwitcherProps) {
  const [open, setOpen] = useState(false)
  const runIds = runs.map((row) => row.session.id)
  const targets = switcherTargets(faces, face, runIds, viewedRunId, offerStart)
  const mode = switcherMode(targets)
  const badge = switcherBadge(face, sessionTone, hasChanges)
  const multipleRuns = runIds.length > 1

  const activate = (target: SwitcherTarget) => {
    if (target.kind === `startCoding`) {
      onStart?.()
      return
    }
    if (target.kind === `run`) {
      const row = runs.find((entry) => entry.session.id === target.id)
      if (row) onOpenRun?.(row.session)
      return
    }
    onFace(target.face)
  }

  if (mode.kind === `hidden`) return null

  if (mode.kind === `toggle`) {
    const label = targetLabel(mode.target, multipleRuns)
    return (
      <button
        type="button"
        aria-label={label}
        title={label}
        data-testid="mobile-face-switcher"
        data-face-target={
          mode.target.kind === `face` ? mode.target.face : mode.target.kind
        }
        onClick={() => activate(mode.target)}
        className={cn(MOBILE_WORK_CIRCLE_CLASS, `relative text-foreground`)}
      >
        <TargetIcon target={mode.target} className="size-5" />
        {badge && <BadgeDot tone={badge} />}
      </button>
    )
  }

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          aria-label="Switch view"
          title="Switch view"
          data-testid="mobile-face-switcher"
          data-face-target="menu"
          className={cn(MOBILE_WORK_CIRCLE_CLASS, `relative text-foreground`)}
        >
          {open ? (
            <CloseIcon className="size-5" />
          ) : (
            <FacesIcon className="size-5" />
          )}
          {badge && !open && <BadgeDot tone={badge} />}
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        side="top"
        align="end"
        sideOffset={8}
        data-testid="mobile-face-switcher-menu"
      >
        {mode.targets.map((target) => {
          if (target.kind === `run`) {
            const row = runs.find((entry) => entry.session.id === target.id)
            if (!row) return null
            const live = isLiveRunStatus(row.session.status)
            return (
              <DropdownMenuCheckboxItem
                key={`run-${target.id}`}
                checked={target.id === viewedRunId}
                onSelect={() => activate(target)}
                data-testid={`mobile-face-run-${target.id}`}
              >
                <span
                  aria-hidden
                  className={cn(
                    `size-1.5 shrink-0 rounded-full`,
                    live ? SESSION_DOT_CLASS.running : SESSION_DOT_CLASS.muted
                  )}
                />
                <span className="min-w-0 truncate">
                  {issueRunEntryLabel(row)}
                </span>
              </DropdownMenuCheckboxItem>
            )
          }
          const key =
            target.kind === `startCoding` ? `start` : `face-${target.face}`
          const testId =
            target.kind === `startCoding`
              ? `mobile-face-option-start`
              : `mobile-face-option-${target.face}`
          return (
            <DropdownMenuItem
              key={key}
              onSelect={() => activate(target)}
              data-testid={testId}
            >
              <TargetIcon target={target} className="size-4" />
              <span className="min-w-0 flex-1 truncate">
                {target.kind === `face` && target.face === `changes`
                  ? CHANGES_FACE_LABEL
                  : targetLabel(target, multipleRuns)}
              </span>
              {target.kind === `face` &&
                target.face === `changes` &&
                diffStats && (
                  <span className="ml-3 shrink-0 text-xs">
                    <DiffFaceLabel
                      additions={diffStats.additions}
                      deletions={diffStats.deletions}
                    />
                  </span>
                )}
            </DropdownMenuItem>
          )
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
