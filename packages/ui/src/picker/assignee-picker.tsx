import type { ReactNode } from "react"

import { Picker, type PickerItem } from "./picker"

// EXP-1029 contract — the assignee picker: the team's members by avatar +
// name, the email as the description and a search keyword; `Unassigned`
// first. The issue properties and the create-issue dialog pick one (single
// mode: `null` = unassigned, `allowsNone` offers the `Unassigned` row); the
// board filter picks several (multi mode, `allowsNone` ignored). The SAME
// shape on the IDE, iOS and Android, where a single pick is an empty-or-one
// set.

export interface AssigneePickerMember {
  id: string
  name: string
  email?: string | null
  image?: string | null
}

/** The row that clears the pick. */
export const UNASSIGNED_VALUE = `` as const

interface AssigneePickerBase {
  members: readonly AssigneePickerMember[]
  trigger: ReactNode
  /** The sheet's title on a phone. */
  mobileTitle?: string
  search?: boolean
  disabled?: boolean
  emptyText?: string
  /** Offer the `Unassigned` row (single mode only; ignored in multi). */
  allowsNone?: boolean
  className?: string
}

export type AssigneePickerProps = AssigneePickerBase &
  (
    | { mode?: `single`; value: string | null; onChange: (userId: string | null) => void }
    | { mode: `multi`; value: readonly string[]; onChange: (userIds: string[]) => void }
  )

export function assigneePickerItems(
  members: readonly AssigneePickerMember[],
  allowsNone = false
): PickerItem[] {
  const rows: PickerItem[] = members.map((member) => ({
    value: member.id,
    label: member.name,
    description: member.email ?? undefined,
    keywords: [member.name, member.email ?? ``].filter((k) => k !== ``),
  }))
  return allowsNone ? [{ value: UNASSIGNED_VALUE, label: `Unassigned` }, ...rows] : rows
}

export function AssigneePicker({
  members,
  allowsNone = false,
  emptyText = `No members`,
  mobileTitle = `Assignee`,
  search = true,
  ...props
}: AssigneePickerProps) {
  const shared = { emptyText, mobileTitle, search }
  if (props.mode === `multi`) {
    const { mode: _mode, ...rest } = props
    return (
      <Picker mode="multi" items={assigneePickerItems(members)} {...shared} {...rest} />
    )
  }
  const { mode: _mode, value, onChange, ...rest } = props
  return (
    <Picker
      mode="single"
      items={assigneePickerItems(members, allowsNone)}
      value={value ?? (allowsNone ? UNASSIGNED_VALUE : null)}
      onChange={(picked) => onChange(picked === UNASSIGNED_VALUE ? null : picked)}
      {...shared}
      {...rest}
    />
  )
}
