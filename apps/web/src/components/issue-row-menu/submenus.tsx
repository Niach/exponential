import { Flag, ListTodo, Rocket, Tag, UserRound, X } from "lucide-react"
import type { Issue, Label, Release, User } from "@/db/schema"
import { issuePriorityOptions, issueStatusOptions } from "@/lib/domain"
import { getInitials } from "@/lib/utils"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import {
  ContextMenuCheckboxItem,
  ContextMenuItem,
  ContextMenuRadioGroup,
  ContextMenuRadioItem,
  ContextMenuShortcut,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "@/components/ui/context-menu"

interface StatusSubmenuProps {
  status: Issue[`status`]
  topLevelValueClass: string
  onSelect: (status: Issue[`status`]) => void
}

export function StatusSubmenu({
  status,
  topLevelValueClass,
  onSelect,
}: StatusSubmenuProps) {
  const statusLabel =
    issueStatusOptions.find((option) => option.value === status)?.label ??
    `Status`

  return (
    <ContextMenuSub>
      <ContextMenuSubTrigger>
        <ListTodo className="size-4" />
        Status
        <ContextMenuShortcut className={topLevelValueClass}>
          {statusLabel}
        </ContextMenuShortcut>
      </ContextMenuSubTrigger>
      <ContextMenuSubContent className="w-[14rem]">
        <ContextMenuRadioGroup value={status}>
          {issueStatusOptions.map((option) => {
            const Icon = option.icon

            return (
              <ContextMenuRadioItem
                key={option.value}
                value={option.value}
                onSelect={() => onSelect(option.value)}
              >
                <Icon className={`size-4 ${option.color}`} />
                {option.label}
              </ContextMenuRadioItem>
            )
          })}
        </ContextMenuRadioGroup>
      </ContextMenuSubContent>
    </ContextMenuSub>
  )
}

interface AssigneeSubmenuProps {
  assigneeId: Issue[`assigneeId`]
  orderedUsers: User[]
  selectedAssignee: User | null
  topLevelValueClass: string
  onSelect: (userId: string | null) => void
}

export function AssigneeSubmenu({
  assigneeId,
  orderedUsers,
  selectedAssignee,
  topLevelValueClass,
  onSelect,
}: AssigneeSubmenuProps) {
  return (
    <ContextMenuSub>
      <ContextMenuSubTrigger>
        <UserRound className="size-4" />
        Assignee
        <ContextMenuShortcut className={topLevelValueClass}>
          {selectedAssignee?.name ?? `Unassigned`}
        </ContextMenuShortcut>
      </ContextMenuSubTrigger>
      <ContextMenuSubContent className="w-[15rem]">
        <ContextMenuRadioGroup value={assigneeId ?? `__unassigned__`}>
          <ContextMenuRadioItem
            value="__unassigned__"
            onSelect={() => onSelect(null)}
          >
            <X className="size-4 text-muted-foreground" />
            Unassigned
          </ContextMenuRadioItem>

          {orderedUsers.length === 0 ? (
            <ContextMenuItem disabled inset>
              No team members yet
            </ContextMenuItem>
          ) : (
            orderedUsers.map((user) => (
              <ContextMenuRadioItem
                key={user.id}
                value={user.id}
                onSelect={() => onSelect(user.id)}
              >
                <Avatar className="size-5">
                  {user.image && (
                    <AvatarImage src={user.image} alt={user.name} />
                  )}
                  <AvatarFallback className="text-[0.5625rem]">
                    {getInitials(user.name)}
                  </AvatarFallback>
                </Avatar>
                <span className="truncate">{user.name}</span>
              </ContextMenuRadioItem>
            ))
          )}
        </ContextMenuRadioGroup>
      </ContextMenuSubContent>
    </ContextMenuSub>
  )
}

interface PrioritySubmenuProps {
  priority: Issue[`priority`]
  topLevelValueClass: string
  onSelect: (priority: Issue[`priority`]) => void
}

export function PrioritySubmenu({
  priority,
  topLevelValueClass,
  onSelect,
}: PrioritySubmenuProps) {
  const priorityLabel =
    issuePriorityOptions.find((option) => option.value === priority)?.label ??
    `Priority`

  return (
    <ContextMenuSub>
      <ContextMenuSubTrigger>
        <Flag className="size-4" />
        Priority
        <ContextMenuShortcut className={topLevelValueClass}>
          {priorityLabel}
        </ContextMenuShortcut>
      </ContextMenuSubTrigger>
      <ContextMenuSubContent className="w-[14rem]">
        <ContextMenuRadioGroup value={priority}>
          {issuePriorityOptions.map((option) => {
            const Icon = option.icon

            return (
              <ContextMenuRadioItem
                key={option.value}
                value={option.value}
                onSelect={() => onSelect(option.value)}
              >
                <Icon className={`size-4 ${option.color}`} />
                {option.label}
              </ContextMenuRadioItem>
            )
          })}
        </ContextMenuRadioGroup>
      </ContextMenuSubContent>
    </ContextMenuSub>
  )
}

interface ReleaseSubmenuProps {
  releaseId: Issue[`releaseId`]
  // Workspace releases, already sorted by compareReleases. Kept a plain prop
  // (no live query here) so this stays presentational like its siblings.
  releases: Release[]
  topLevelValueClass: string
  onSelect: (releaseId: string | null) => void
}

export function ReleaseSubmenu({
  releaseId,
  releases,
  topLevelValueClass,
  onSelect,
}: ReleaseSubmenuProps) {
  const currentName = releaseId
    ? releases.find((release) => release.id === releaseId)?.name
    : undefined

  return (
    <ContextMenuSub>
      <ContextMenuSubTrigger>
        <Rocket className="size-4" />
        Add to release
        <ContextMenuShortcut className={topLevelValueClass}>
          {currentName ?? `None`}
        </ContextMenuShortcut>
      </ContextMenuSubTrigger>
      <ContextMenuSubContent className="w-[15rem]">
        <ContextMenuRadioGroup value={releaseId ?? `__none__`}>
          <ContextMenuRadioItem
            value="__none__"
            onSelect={() => onSelect(null)}
          >
            <X className="size-4 text-muted-foreground" />
            No release
          </ContextMenuRadioItem>

          {releases.length === 0 ? (
            <ContextMenuItem disabled inset>
              No releases yet
            </ContextMenuItem>
          ) : (
            releases.map((release) => (
              <ContextMenuRadioItem
                key={release.id}
                value={release.id}
                onSelect={() => onSelect(release.id)}
              >
                <Rocket className="size-4 text-muted-foreground" />
                <span className="truncate">{release.name}</span>
              </ContextMenuRadioItem>
            ))
          )}
        </ContextMenuRadioGroup>
      </ContextMenuSubContent>
    </ContextMenuSub>
  )
}

interface LabelsSubmenuProps {
  labels: Label[]
  selectedLabelIds: Set<string>
  topLevelValueClass: string
  onToggle: (labelId: string) => void
}

export function LabelsSubmenu({
  labels,
  selectedLabelIds,
  topLevelValueClass,
  onToggle,
}: LabelsSubmenuProps) {
  const labelsLabel =
    selectedLabelIds.size > 0 ? `${selectedLabelIds.size} selected` : `None`

  return (
    <ContextMenuSub>
      <ContextMenuSubTrigger>
        <Tag className="size-4" />
        Labels
        <ContextMenuShortcut className={topLevelValueClass}>
          {labelsLabel}
        </ContextMenuShortcut>
      </ContextMenuSubTrigger>
      <ContextMenuSubContent className="w-[15rem]">
        {labels.length === 0 ? (
          <ContextMenuItem disabled inset>
            No labels yet
          </ContextMenuItem>
        ) : (
          labels.map((label) => (
            <ContextMenuCheckboxItem
              key={label.id}
              checked={selectedLabelIds.has(label.id)}
              onSelect={(event) => {
                event.preventDefault()
                onToggle(label.id)
              }}
            >
              <div
                className="size-2.5 rounded-full shrink-0"
                style={{ backgroundColor: label.color }}
              />
              <span className="truncate">{label.name}</span>
            </ContextMenuCheckboxItem>
          ))
        )}
      </ContextMenuSubContent>
    </ContextMenuSub>
  )
}
