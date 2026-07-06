/* ─── Small shared atoms: status/priority icons, avatar, label chip, tool header ─── */
import type { ReactNode } from "react"
import type { Assignee, IssuePriority, IssueStatus, Label } from "./data"
import {
  IcAlert,
  IcCircle,
  IcCircleCheck,
  IcCircleDashed,
  IcMinus,
  IcSigHigh,
  IcSigLow,
  IcSigMed,
  IcTimer,
  IcUser,
} from "./icons"

export function StatusIcon({ status, size = 14 }: { status: IssueStatus; size?: number }) {
  switch (status) {
    case `backlog`:
      return <IcCircleDashed size={size} className="ide-c-muted" />
    case `todo`:
      return <IcCircle size={size} className="ide-c-fg" />
    case `in_progress`:
      return <IcTimer size={size} className="ide-c-yellow" />
    case `done`:
      return <IcCircleCheck size={size} className="ide-c-green" />
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
      style={{ width: size, height: size, fontSize: Math.round(size * 0.45) }}
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
