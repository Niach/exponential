import type { ReactNode } from "react"
import { assigneeOptions } from "@exp/domain-contract/agent-settings"

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
//
// #EXP-1383 (agent-settings.json `assignee`): after the members, the team's
// AGENTS (`agents`, ordered by `assigneeOptions`: sortOrder, then name), each
// row = THE AGENT MARK (`UserAvatar agent`, the Chats bubble's glyph on the
// muted disc) + the name, which search matches. An agent row's value is
// `agent:<id>` (`assigneeAgentValue`); a member's stays the bare user id, so
// `assigneeFromPickerValue` splits a pick into the two exclusive columns.

/** #EXP-1383: an agent row's value prefix (`assigneeOptions` ids). */
export const ASSIGNEE_AGENT_PREFIX = `agent:`

export function assigneeAgentValue(agentId: string): string {
  return `${ASSIGNEE_AGENT_PREFIX}${agentId}`
}

/** The single picker's value for an issue's two exclusive columns. */
export function assigneePickerValue(input: {
  assigneeId: string | null | undefined
  assigneeAgentId?: string | null | undefined
}): string | null {
  if (input.assigneeAgentId) return assigneeAgentValue(input.assigneeAgentId)
  return input.assigneeId ?? null
}

/** A pick back into the columns: an agent clears the person, a person (or
 *  `Unassigned`) clears the agent, both explicit. */
export function assigneeFromPickerValue(value: string | null): {
  assigneeId: string | null
  assigneeAgentId: string | null
} {
  if (value?.startsWith(ASSIGNEE_AGENT_PREFIX)) {
    return { assigneeId: null, assigneeAgentId: value.slice(ASSIGNEE_AGENT_PREFIX.length) }
  }
  return { assigneeId: value, assigneeAgentId: null }
}

export interface AssigneePickerMember {
  id: string
  name: string
  email?: string | null
  image?: string | null
}

/** #EXP-1383: a team agent as an assignee row. */
export interface AssigneePickerAgent {
  id: string
  name: string
  icon: string | null
  color: string | null
  sortOrder: number
}

interface AssigneePickerBase extends PickerSurfaceProps {
  members: readonly AssigneePickerMember[]
  /** #EXP-1383: the team's agents, listed after the members. */
  agents?: readonly AssigneePickerAgent[]
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
  members: readonly AssigneePickerMember[],
  agents: readonly AssigneePickerAgent[] = []
): PickerItem[] {
  const memberItems = members.map((member) => ({
    value: member.id,
    label: member.name,
    description: member.email ?? undefined,
    keywords: [member.name, member.email ?? ``].filter((k) => k !== ``),
  }))
  if (agents.length === 0) return memberItems
  const agentById = new Map(agents.map((agent) => [agent.id, agent]))
  const agentItems = assigneeOptions({ members: [], agents })
    .filter((id) => id.startsWith(ASSIGNEE_AGENT_PREFIX))
    .map((id) => agentById.get(id.slice(ASSIGNEE_AGENT_PREFIX.length)))
    .filter((agent): agent is AssigneePickerAgent => agent !== undefined)
    .map((agent) => ({
      value: assigneeAgentValue(agent.id),
      label: agent.name,
      keywords: [agent.name],
    }))
  return [...memberItems, ...agentItems]
}

export function AssigneePicker({
  members,
  agents = [],
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
  const agentByValue = new Map(agents.map((agent) => [assigneeAgentValue(agent.id), agent]))
  const renderItem = (item: PickerItem) => {
    const agent = agentByValue.get(item.value)
    if (agent) {
      return (
        <>
          <UserAvatar size={20} user={null} agent={agent} />
          <PickerItemBody item={item} />
        </>
      )
    }
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
  const items = assigneePickerItems(members, agents)
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
