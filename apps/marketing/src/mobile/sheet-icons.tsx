/* ─── Agent page glyphs — the shared registry, iOS sizing ───
   The subject chips wear the SAME glyphs the phone does (packages/icons
   icons.json, the status / priority / ui-selected names): lucide marks plus
   the two pie clocks, which are registry-only shapes (`progress-2-4` / `progress-3-4`,
   path data copied verbatim). The IDE recreation's own StatusIcon speaks the
   desktop's timer/PR vocabulary, so the phone keeps its own atoms rather than
   drifting the shared ones. Colors are the design-token semantics
   (packages/design-tokens tokens.json + ExpUI StatusColor/PriorityColor).
   Sizes are iOS POINTS on the 414pt canvas — see AgentComposer.tsx. */
import {
  CircleCheck,
  CircleDashed,
  Minus,
  SignalHigh,
  SignalLow,
  SignalMedium,
  TriangleAlert,
} from "lucide-react"
import type { IssuePriority, IssueStatus } from "../ide/data"

const NEUTRAL = `#a1a1aa`
const YELLOW = `#facc15`
const GREEN = `#22c55e`
const RED = `#ef4444`
const ORANGE = `#f97316`
const BLUE = `#3b82f6`

/* The registry's pie clocks: a lucide-geometry ring plus a filled wedge. */
function ProgressClock({
  wedge,
  size,
  color,
}: {
  wedge: string
  size: number
  color: string
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={color}
      strokeWidth={2}
      aria-hidden
      className={`mag-glyph mag-status`}
    >
      <circle cx="12" cy="12" r="10" />
      <path d={wedge} fill={color} stroke="none" />
    </svg>
  )
}

export function MagStatusIcon({ status }: { status: IssueStatus }) {
  /* AppIcon.Size.small on the phone, measured at 15pt of drawn glyph. */
  const size = 15
  const props = { size, strokeWidth: 2, "aria-hidden": true } as const
  switch (status) {
    case `backlog`:
      return <CircleDashed {...props} color={NEUTRAL} className={`mag-glyph mag-status`} />
    case `in_progress`:
      return (
        <ProgressClock
          size={size}
          color={YELLOW}
          wedge="M12 12 L12 6 A6 6 0 0 1 12 18 Z"
        />
      )
    case `in_review`:
      return (
        <ProgressClock
          size={size}
          color={GREEN}
          wedge="M12 12 L12 6 A6 6 0 1 1 6 12 Z"
        />
      )
    case `done`:
      return <CircleCheck {...props} color={BLUE} className={`mag-glyph mag-status`} />
  }
}

export function MagPriorityIcon({ priority }: { priority: IssuePriority }) {
  const props = { size: 13, strokeWidth: 2, "aria-hidden": true } as const
  switch (priority) {
    case `none`:
      return <Minus {...props} color={NEUTRAL} className={`mag-glyph mag-prio`} />
    case `urgent`:
      return <TriangleAlert {...props} color={RED} className={`mag-glyph mag-prio`} />
    case `high`:
      return <SignalHigh {...props} color={ORANGE} className={`mag-glyph mag-prio`} />
    case `medium`:
      return <SignalMedium {...props} color={YELLOW} className={`mag-glyph mag-prio`} />
    case `low`:
      return <SignalLow {...props} color={BLUE} className={`mag-glyph mag-prio`} />
  }
}
