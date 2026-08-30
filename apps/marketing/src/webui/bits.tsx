/* ─── Web-app atoms ───
   The IDE's atoms (ide/bits.tsx) render the desktop app's smaller, softer
   glyph set; the web app draws lucide at full stroke plus the EXP-314
   positional pie clocks for the `started` category, so the web recreation
   carries its own. Colors are the contract's builtin status hexes
   (packages/domain-contract/contract.json → issueStatusDefaults) and the
   Tailwind v4 priority hues from apps/web lib/domain.ts. */
import type { Assignee, IssuePriority, IssueStatus, Label } from "../ide/data"
import {
  ICON_35,
  IcCircleCheck,
  IcCircleDashed,
  IcMinus,
  IcSignalHigh,
  IcSignalLow,
  IcSignalMedium,
  IcTriangleAlert,
  IcUser,
} from "./icons"

/* Builtin status colors — contract issueStatusDefaults. */
export const STATUS_COLOR: Record<IssueStatus, string> = {
  backlog: `#A1A1AA`,
  in_progress: `#EAB308`,
  in_review: `#22C55E`,
  done: `#3B82F6`,
}

/* Group-header washes — the `/10` alpha of the Tailwind token each builtin
   header uses (zinc-500 / yellow-500 / green-500 / blue-500). */
export const STATUS_WASH: Record<IssueStatus, string> = {
  backlog: `rgba(113, 113, 122, 0.1)`,
  in_progress: `rgba(240, 177, 0, 0.1)`,
  in_review: `rgba(0, 201, 81, 0.1)`,
  done: `rgba(43, 127, 255, 0.1)`,
}

/* Tailwind v4 OKLCH palette, flattened. */
const PRIORITY_COLOR: Record<IssuePriority, string> = {
  none: `#a1a1a1`,
  urgent: `#fb2c36`,
  high: `#ff6900`,
  medium: `#f0b100`,
  low: `#2b7fff`,
}

/* EXP-314 pie clocks: with the two builtin started statuses the pair is
   [2/4, 3/4] — the wedge paths are lifted verbatim from icons.json. */
const CLOCK_WEDGE: Record<string, string> = {
  in_progress: `M12 12 L12 6 A6 6 0 0 1 12 18 Z`,
  in_review: `M12 12 L12 6 A6 6 0 1 1 6 12 Z`,
}

function ProgressClock({ status, size }: { status: IssueStatus; size: number }) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ color: STATUS_COLOR[status] }}
    >
      <circle cx="12" cy="12" r="10" />
      <path d={CLOCK_WEDGE[status]} fill="currentColor" stroke="none" />
    </svg>
  )
}

export function StatusGlyph({
  status,
  size = ICON_35,
}: {
  status: IssueStatus
  size?: number
}) {
  const style = { color: STATUS_COLOR[status] }
  switch (status) {
    case `backlog`:
      return <IcCircleDashed size={size} style={style} />
    case `done`:
      return <IcCircleCheck size={size} style={style} />
    default:
      return <ProgressClock status={status} size={size} />
  }
}

export function PriorityGlyph({
  priority,
  size = ICON_35,
}: {
  priority: IssuePriority
  size?: number
}) {
  const style = { color: PRIORITY_COLOR[priority] }
  switch (priority) {
    case `urgent`:
      return <IcTriangleAlert size={size} style={style} />
    case `high`:
      return <IcSignalHigh size={size} style={style} />
    case `medium`:
      return <IcSignalMedium size={size} style={style} />
    case `low`:
      return <IcSignalLow size={size} style={style} />
    default:
      return <IcMinus size={size} style={style} />
  }
}

/* shadcn <Avatar> with the initials fallback; unassigned renders the dashed
   placeholder circle from assignee-dropdown.tsx. */
export function WebAvatar({
  person,
  size = 23.125,
}: {
  person?: Assignee
  size?: number
}) {
  if (!person) {
    return (
      <span className="web-avatar is-empty" style={{ width: size, height: size }}>
        <IcUser size={size * 0.5} />
      </span>
    )
  }
  return (
    <span
      className="web-avatar"
      title={person.name}
      style={{ width: size, height: size, fontSize: size * 0.5 }}
    >
      {person.initials}
    </span>
  )
}

/* Row label pill — outlined capsule with a color dot (issue-list.tsx). */
export function LabelPill({ label }: { label: Label }) {
  return (
    <span className="web-label">
      <span className="web-label-dot" style={{ background: label.color }} />
      {label.name}
    </span>
  )
}
