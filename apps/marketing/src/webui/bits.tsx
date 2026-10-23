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

/* Builtin status glyph colours = @exp/ui BUILTIN_STATUS_COLOR_CLASS
   (status-icons.ts): text-muted-foreground, then the Tailwind v4 yellow-500 /
   green-500 / blue-500, flattened to hex. */
export const STATUS_COLOR: Record<IssueStatus, string> = {
  backlog: `#a1a1a1`,
  in_progress: `#f0b100`,
  in_review: `#00c950`,
  done: `#2b7fff`,
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

/* @exp/ui UserAvatar: the initials fallback in the person's hue
   (avatar-color.ts: fnv1a32(userId) % 8 over tokens.json `avatar`, the
   fallback = hue at 20% behind the hue as text). The demo keys the hue off
   the name. */
const AVATAR_HUE_COUNT = 8
const FNV_OFFSET_BASIS = 0x811c9dc5
const FNV_PRIME = 0x01000193
const encoder = new TextEncoder()

/* The SDK's contract verbatim (packages/exponential-ui/src/web/primitives/
   avatar-color.ts, pinned ×4 by avatar-color.test.ts): FNV-1a 32-bit over
   the UTF-8 bytes, modulo the palette. Copied rather than imported so the
   mock does not pin the SDK package's npm scope. */
function avatarHueIndex(key: string): number {
  let hash = FNV_OFFSET_BASIS
  for (const byte of encoder.encode(key)) {
    hash ^= byte
    hash = Math.imul(hash, FNV_PRIME) >>> 0
  }
  return hash % AVATAR_HUE_COUNT
}

export function avatarHue(key: string): string {
  return `var(--avatar-${avatarHueIndex(key)})`
}

export function WebUserAvatar({
  name,
  initials,
  size,
  fontSize,
}: {
  name: string
  initials: string
  size: number
  fontSize: number
}) {
  const hue = avatarHue(name)
  return (
    <span
      className="web-avatar"
      title={name}
      style={{
        width: size,
        height: size,
        fontSize,
        backgroundColor: `color-mix(in srgb, ${hue} 20%, transparent)`,
        color: hue,
      }}
    >
      {initials}
    </span>
  )
}

/* The issue row's assignee cell (issue-list.tsx AssigneeCell): UserAvatar
   size 20 (size-5, text-[0.5625rem]); unassigned = the dashed size-5 circle
   with the size-2.5 placeholder glyph at muted/50. */
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
        <IcUser size={size / 2} />
      </span>
    )
  }
  return (
    <WebUserAvatar
      name={person.name}
      initials={person.initials}
      size={size}
      fontSize={size * 0.45}
    />
  )
}

/* Row label = the @exp/ui Pill (size sm, readonly) with its dot: h-6, gap-1,
   px-2, text-xs, font-medium, the glass card fill + stroke, fg/70. */
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

/* @exp/ui AgentRunMark (agent-brand-mark.tsx): the agent's brand mark in a
   size-3.5 box; `working` = Claude's spark spinner (drawn here as its
   reduced-motion frame, the static mark) or Codex's pulsing mark; `review`
   = the mark + an emerald-500 size-1.5 badge at the top right, ring-2 in the
   sidebar colour. */
export function AgentRunMarkDemo({ agent, state }: { agent: DemoAgent; state: RunState }) {
  return (
    <span className="web-runmark">
      <AgentMark agent={agent} size={ICON_35} />
      {state === `review` && <span className="web-runmark-badge" />}
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
