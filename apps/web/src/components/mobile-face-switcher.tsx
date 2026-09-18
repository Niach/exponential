import { useState } from "react"
import type { CodingSession } from "@/db/schema"
import {
  conceptIcon,
  ComboboxMenuItems,
  DiffCounts,
  SESSION_DOT_CLASS,
  type SessionDotTone,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  MOBILE_WORK_CIRCLE_CLASS,
} from "@exp/ui"
import { isLiveRun } from "@/lib/past-runs"
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
import {
  issueRunOption,
  RunSessionDot,
} from "@/components/issue-run-switcher"

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
//
// EXP-958: the run rows are the Combobox's menu arm (`ComboboxMenuItems`),
// the same rows the desktop-width run switcher draws: a single select with
// the picker's trailing check on the run on show, never the dropdown's own
// checkbox tick. `switcherTargets` keeps the run targets contiguous, so the
// first one renders the whole block and the rest are skipped.

const IssueIcon = conceptIcon(`ui-issue`)
const RunIcon = conceptIcon(`nav-devices`)
const ChangesIcon = conceptIcon(`coding-diff`)
/** EXP-879: the run's published screenshots. */
const ResultsIcon = conceptIcon(`work-results`)
const StartIcon = conceptIcon(`action-run`)
const FacesIcon = conceptIcon(`work-faces`)
const CloseIcon = conceptIcon(`ui-close`)

const FACE_ICON: Record<WorkFaceKind, typeof IssueIcon> = {
  issue: IssueIcon,
  run: RunIcon,
  changes: ChangesIcon,
  results: ResultsIcon,
}

/** EXP-931: where the switcher is standing. `circle` is the work bar's 52px
 *  glass slot; `inline` is the EXPANDED composer's control row, where it sits
 *  beside the usage ring and wears exactly the ring's chrome — the bar (and
 *  its circle) are gone while the composer is open, and the linked Issue /
 *  Changes / Results must stay reachable. */
export type MobileFaceSwitcherVariant = `circle` | `inline`

export interface MobileFaceSwitcherProps {
  /** The faces this subject has (`availableFaces`). */
  faces: readonly WorkFaceKind[]
  variant?: MobileFaceSwitcherVariant
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

/** EXP-916: the dot rides the GLYPH's top-trailing corner (3px out), on its
 *  own 12px opaque disc — Android's `FaceSwitcher` badge, inside the circle
 *  rather than hanging off its edge. */
function BadgeDot({ tone }: { tone: SessionDotTone | `changes` }) {
  return (
    <span
      aria-hidden
      data-testid="mobile-face-switcher-badge"
      data-tone={tone}
      className={cn(
        `absolute -right-[3px] -top-[3px] size-2 rounded-full ring-2 ring-popover`,
        tone === `changes` ? SESSION_DOT_CLASS.running : SESSION_DOT_CLASS[tone]
      )}
    />
  )
}

export function MobileFaceSwitcher({
  faces,
  variant = `circle`,
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
  // EXP-931: the composer's row is 16px controls (`ContextRing` is a ghost
  // `icon-xs` Button), the bar's is the 52px glass circle.
  const inline = variant === `inline`
  const buttonClass = inline
    ? `inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors duration-fast hover:bg-glass-active hover:text-foreground`
    : cn(MOBILE_WORK_CIRCLE_CLASS, `text-foreground`)
  const glyphClass = inline ? `size-4` : `size-5`

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
        className={buttonClass}
      >
        <span className="relative flex">
          <TargetIcon target={mode.target} className={glyphClass} />
          {badge && <BadgeDot tone={badge} />}
        </span>
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
          className={buttonClass}
        >
          <span className="relative flex">
            {open ? (
              <CloseIcon className={glyphClass} />
            ) : (
              <FacesIcon className={glyphClass} />
            )}
            {badge && !open && <BadgeDot tone={badge} />}
          </span>
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        side="top"
        align="end"
        sideOffset={8}
        data-testid="mobile-face-switcher-menu"
      >
        {mode.targets.map((target, index) => {
          if (target.kind === `run`) {
            const first = mode.targets.findIndex((entry) => entry.kind === `run`)
            if (index !== first) return null
            const runRows = mode.targets.flatMap((entry) =>
              entry.kind === `run`
                ? runs.filter((row) => row.session.id === entry.id)
                : []
            )
            return (
              <ComboboxMenuItems
                key="runs"
                menu="dropdown"
                options={runRows.map(issueRunOption)}
                value={viewedRunId}
                onChange={(id) => {
                  if (id !== null) activate({ kind: `run`, id })
                }}
                renderOption={(option) => (
                  <>
                    <RunSessionDot
                      live={isLiveRun(
                        runRows.find((row) => row.session.id === option.value)!
                          .session
                      )}
                    />
                    <span className="min-w-0 truncate">{option.label}</span>
                  </>
                )}
              />
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
                    <DiffCounts
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
