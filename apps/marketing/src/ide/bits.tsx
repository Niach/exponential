/* ─── Small shared atoms: status/priority icons, avatar, label chip, tool header ─── */
import type { ReactNode } from "react"
import type { Assignee, IssuePriority, IssueStatus, Label } from "./data"
import {
  IcAlert,
  IcCircleCheck,
  IcCircleDashed,
  IcMinus,
  IcProgress24,
  IcProgress34,
  IcSigHigh,
  IcSigLow,
  IcSigMed,
  IcUser,
} from "./icons"

export function StatusIcon({ status, size = 14 }: { status: IssueStatus; size?: number }) {
  switch (status) {
    /* EXP-314 category glyphs: backlog dashed ring, started = the positional
       pie clock, completed = check (the unstarted plain ring left with the
       Todo builtin, EXP-685). Colors are the
       contract's `issueStatusDefaults` hexes. */
    case `backlog`:
      return <IcCircleDashed size={size} style={{ color: `#a1a1aa` }} />
    case `in_progress`:
      return <IcProgress24 size={size} style={{ color: `#eab308` }} />
    case `in_review`:
      return <IcProgress34 size={size} style={{ color: `#22c55e` }} />
    case `done`:
      return <IcCircleCheck size={size} style={{ color: `#3b82f6` }} />
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

/* gpui_component's Avatar hue-hashes the display name when no profile image
   has landed — the same stable per-person tint web/iOS/Android paint. */
const avatarHue = (name: string): number => {
  let hash = 0
  for (let i = 0; i < name.length; i += 1) {
    hash = (hash * 31 + name.charCodeAt(i)) >>> 0
  }
  return hash % 360
}

export function Avatar({ person, size = 16 }: { person?: Assignee; size?: number }) {
  if (!person) {
    return (
      <span className="ide-avatar-empty" style={{ width: size, height: size }}>
        <IcUser size={Math.round(size * 0.55)} />
      </span>
    )
  }
  return (
    <span
      className="ide-avatar"
      title={person.name}
      style={{
        width: size,
        height: size,
        fontSize: Math.max(Math.round(size * 0.44), 7),
        background: `hsl(${avatarHue(person.name)} 48% 42%)`,
      }}
    >
      {person.initials}
    </span>
  )
}

export function LabelChip({ label }: { label: Label }) {
  return (
    <span className="ide-chip">
      <span className="ide-chip-dot" style={{ background: label.color }} />
      {label.name}
    </span>
  )
}

export function ToolHead({
  icon,
  title,
  trailing,
}: {
  icon: ReactNode
  title: string
  trailing?: ReactNode
}) {
  return (
    <div className="ide-toolhead">
      {icon}
      <span className="ide-toolhead-title">{title}</span>
      <div className="ide-flex1" />
      {trailing}
    </div>
  )
}
