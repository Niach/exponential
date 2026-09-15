/* ─── Web-app atoms ───
   The IDE's atoms (ide/bits.tsx) render the desktop app's smaller, softer
   glyph set; the web app draws lucide at full stroke plus the EXP-314
   positional pie clocks for the `started` category, so the web recreation
   carries its own. Colors are the contract's builtin status hexes
   (packages/domain-contract/contract.json → issueStatusDefaults) and the
   Tailwind v4 priority hues from apps/web lib/domain.ts.

   EXP-903: these are DRAWINGS inside a fake-app recreation, at marketing's own
   scale and hex tokens — their product counterparts are `@exp/ui`'s
   `<StatusGlyph>`, `<UserAvatar>` and `<LiveDot>`, and an island (UiDemo)
   replaces one only where a docs page shows the control ITSELF. */
import type { Assignee, IssuePriority, IssueStatus, Label } from "../ide/data"
import { ClaudeLogo, CodexLogo } from "../components/agent-icons"
import type { DemoAgent, RunState } from "./data"
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

/* EXP-877: Claude's REAL mark in its brand orange on every client
   (brand-icons.tsx CLAUDE_FILL #D97757); Codex stays foreground-tinted. */
export const CLAUDE_FILL = `#D97757`

export function AgentMark({ agent, size = ICON_35 }: { agent: DemoAgent; size?: number }) {
  return agent === `claude` ? (
    <span className="web-agentmark" style={{ color: CLAUDE_FILL }}>
      <ClaudeLogo size={size} />
    </span>
  ) : (
    <span className="web-agentmark">
      <CodexLogo size={size} />
    </span>
  )
}

/* RunningIndicator (agent-session-row.tsx): the run's state dot — green and
   PINGING while the agent works (EXP-848 agent_busy), steady green once its
   PR is open. */
export function RunDot({ state }: { state: RunState }) {
  return <span className={`web-rundot${state === `working` ? ` is-working` : ``}`} />
}

/* The ContextRing (context-ring.tsx): a 16px radial, r=6, stroke 2, the
   track at 20% — muted under 75%, amber from 75%, red from 95%. */
export function ContextRing({ percent }: { percent: number }) {
  const r = 6
  const c = 2 * Math.PI * r
  const tone = percent >= 95 ? `#ff6467` : percent >= 75 ? `#fe9a00` : `#a1a1a1`
  return (
    <span className="web-ctxring" title={`Context ${percent}%`} style={{ color: tone }}>
      <svg viewBox="0 0 16 16" width={16} height={16} style={{ transform: `rotate(-90deg)` }}>
        <circle cx={8} cy={8} r={r} fill="none" stroke="currentColor" strokeWidth={2} opacity={0.2} />
        <circle
          cx={8}
          cy={8}
          r={r}
          fill="none"
          stroke="currentColor"
          strokeWidth={2}
          strokeLinecap="round"
          strokeDasharray={c}
          strokeDashoffset={c * (1 - percent / 100)}
        />
      </svg>
    </span>
  )
}
