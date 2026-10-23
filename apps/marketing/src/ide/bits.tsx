/* ─── Small shared atoms: status/priority glyphs, avatars, the label pill,
   the run mark ───
   EXP-903: these are DRAWINGS inside a fake-IDE recreation, at marketing's own
   scale and hex tokens — their product counterparts are the desktop's
   `icons::resolved_status_icon`, `user_avatar` and `run_rows::run_mark`. ─── */
import type { Assignee, IssuePriority, IssueStatus, Label } from "./data"
import {
  IcAlert,
  IcCircleCheck,
  IcCircleDashed,
  IcClaude,
  IcMinus,
  IcProgress24,
  IcProgress34,
  IcSigHigh,
  IcSigLow,
  IcSigMed,
  IcUser,
} from "./icons"

/* The theme's status accents (packages/design-tokens): backlog = muted,
   the builtin started pair yellow / green, completed blue. */
export const STATUS_COLOR: Record<IssueStatus, string> = {
  backlog: `#a1a1a1`,
  in_progress: `#facc15`,
  in_review: `#22c55e`,
  done: `#3b82f6`,
}

/* issue_list.rs `status_header_tint`: the band wash = the status hue at 7%
   (backlog → the NEUTRAL accent). */
export const STATUS_BAND: Record<IssueStatus, string> = {
  backlog: `rgba(161, 161, 170, 0.07)`,
  in_progress: `rgba(250, 204, 21, 0.07)`,
  in_review: `rgba(34, 197, 94, 0.07)`,
  done: `rgba(59, 130, 246, 0.07)`,
}

export function StatusIcon({ status, size = 14 }: { status: IssueStatus; size?: number }) {
  const style = { color: STATUS_COLOR[status] }
  switch (status) {
    /* EXP-314 category glyphs: backlog dashed ring, started = the positional
       pie clock, completed = check. */
    case `backlog`:
      return <IcCircleDashed size={size} style={style} />
    case `in_progress`:
      return <IcProgress24 size={size} style={style} />
    case `in_review`:
      return <IcProgress34 size={size} style={style} />
    case `done`:
      return <IcCircleCheck size={size} style={style} />
  }
}

export function PriorityIcon({ priority, size = 14 }: { priority: IssuePriority; size?: number }) {
  switch (priority) {
    case `none`:
      return <IcMinus size={size} className="ide-c-muted" />
    case `urgent`:
      return <IcAlert size={size} className="ide-c-red" />
    case `high`:
      return <IcSigHigh size={size} className="ide-c-orange" />
    case `medium`:
      return <IcSigMed size={size} className="ide-c-yellow" />
    case `low`:
      return <IcSigLow size={size} className="ide-c-blue" />
  }
}

/* user_avatar.rs `initials_avatar`: the contract hue (design-tokens
   `avatar.HUES`, index = fnv1a32(id) % 8) at 20% under the initials at
   full. The demo hashes the display name, its only stable id. */
const AVATAR_HUES = [`#f87171`, `#fb923c`, `#facc15`, `#4ade80`, `#2dd4bf`, `#60a5fa`, `#a78bfa`, `#f472b6`]

const fnv1a32 = (text: string): number => {
  let hash = 0x811c9dc5
  for (let i = 0; i < text.length; i += 1) {
    hash ^= text.charCodeAt(i)
    hash = Math.imul(hash, 0x01000193) >>> 0
  }
  return hash >>> 0
}

export const avatarHue = (name: string): string => AVATAR_HUES[fnv1a32(name) % AVATAR_HUES.length]

/* `Size::XSmall` = 16px / 0.65rem initials, `Size::Small` = 24px / text_xs. */
export function Avatar({ person, size = 16 }: { person?: Assignee; size?: 16 | 24 }) {
  if (!person) {
    /* The unassigned cell: a dashed placeholder circle with the person glyph. */
    return (
      <span className="ide-avatar-empty" style={{ width: size, height: size }}>
        <IcUser size={size === 16 ? 9 : 12} />
      </span>
    )
  }
  const hue = avatarHue(person.name)
  return (
    <span
      className={`ide-avatar is-${size}`}
      title={person.name}
      style={{ color: hue, background: `color-mix(in srgb, ${hue} 20%, transparent)` }}
    >
      {person.initials}
    </span>
  )
}

/* issue_list.rs `label_chip`: the Sm readonly glass pill, the 6px dot. */
export function LabelChip({ label }: { label: Label }) {
  return (
    <span className="ide-pill is-sm is-readonly">
      <span className="ide-pill-dot" style={{ background: label.color }} />
      {label.name}
    </span>
  )
}

/* run_rows.rs `run_mark`: the agent's mark, with the state's 6px badge on
   its corner (amber = needs you). */
export function RunMark({ needsYou = false, size = 14 }: { needsYou?: boolean; size?: number }) {
  return (
    <span className="ide-runmark" style={{ width: size, height: size }}>
      <IcClaude size={size} />
      {needsYou && <span className="ide-runmark-badge" />}
    </span>
  )
}
