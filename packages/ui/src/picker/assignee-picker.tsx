import type { ReactNode } from "react"

import { UserAvatar } from "../user-avatar"
import {
  Picker,
  PickerItemBody,
  type PickerItem,
  type PickerSurfaceProps,
} from "./picker"

// EXP-1029 contract — the assignee picker: the team's members by avatar +
// name, the email as the description and a search keyword; `Unassigned`
// first. The issue properties and the create-issue dialog pick one (single
// mode: `null` = unassigned, `allowsNone` offers the `Unassigned` row); the
// board filter picks several (multi mode, `allowsNone` ignored). The SAME
// shape on the IDE, iOS and Android, where a single pick is an empty-or-one
// set.
//
// `Unassigned` is the primitive's `noneLabel` row, never an item: an item
// needs a value, and the empty string that stood in for "nobody" was what
// cmdk reads as NO value — nothing highlighted on open, ↓ landing on it
// without Enter ever picking it, and the filter dropping it for `Unass`.
// The none row reports `null` through `onChange` here, no sentinel anywhere.

export interface AssigneePickerMember {
  id: string
  name: string
  email?: string | null
  image?: string | null
}

interface AssigneePickerBase extends PickerSurfaceProps {
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
    | {
        mode: `multi`
        value: readonly string[]
        onChange: (userIds: string[]) => void
        /** At the cap the unpicked rows go disabled; picked ones still
         *  toggle off (the automation trigger's ten-id filters). */
        max?: number
      }
  )

export function assigneePickerItems(
  members: readonly AssigneePickerMember[]
): PickerItem[] {
  return members.map((member) => ({
    value: member.id,
    label: member.name,
    description: member.email ?? undefined,
    keywords: [member.name, member.email ?? ``].filter((k) => k !== ``),
  }))
}

export function AssigneePicker({
  members,
  allowsNone = false,
  emptyText = `No members`,
  mobileTitle = `Assignee`,
  search = true,
  ...props
}: AssigneePickerProps) {
  // The avatar is what makes a member row a MEMBER row, and it is not a
  // `PickerItem` slot (a picker glyph is an icon, never a photo) — so the
  // one row body that needs one draws it here, over the primitive's.
  // `Unassigned` is the primitive's own none row: there is nobody to picture.
  const byId = new Map(members.map((member) => [member.id, member]))
  const renderItem = (item: PickerItem) => {
    const member = byId.get(item.value)
    if (!member) return <PickerItemBody item={item} />
    return (
      <>
        <UserAvatar size={20} user={member} />
        <PickerItemBody item={item} />
      </>
    )
  }
  const shared = { emptyText, mobileTitle, search, renderItem }
  const items = assigneePickerItems(members)
  if (props.mode === `multi`) {
    const { mode: _mode, ...rest } = props
    return <Picker mode="multi" items={items} {...shared} {...rest} />
  }
  const { mode: _mode, value, onChange, ...rest } = props
  return (
    <Picker
      mode="single"
      items={items}
      value={value}
      onChange={onChange}
      {...(allowsNone
        ? { noneLabel: `Unassigned`, onNone: () => onChange(null) }
        : {})}
      {...shared}
      {...rest}
    />
  )
}
