import { FolderInput, Tag, UserRound, X } from "lucide-react"
import type { Issue, Label, Project, User } from "@/db/schema"
import {
  getIssuePriorityConfig,
  getIssueStatusConfig,
  issuePriorityOptions,
  issueStatusOptions,
} from "@/lib/domain"
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
  // Trigger mirrors the row's status icon: the CURRENT status, not a generic
  // glyph (EXP-59).
  const statusConfig = getIssueStatusConfig(status)
  const StatusIcon = statusConfig.icon

  return (
    <ContextMenuSub>
      <ContextMenuSubTrigger>
        <StatusIcon className={`size-4 ${statusConfig.color}`} />
        Status
        <ContextMenuShortcut className={topLevelValueClass}>
          {statusConfig.label}
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
        {/* Current assignee's avatar; person placeholder when unassigned (EXP-59). */}
        {selectedAssignee ? (
          <Avatar className="size-4">
            {selectedAssignee.image && (
              <AvatarImage
                src={selectedAssignee.image}
                alt={selectedAssignee.name}
              />
            )}
            <AvatarFallback className="text-[0.5rem]">
              {getInitials(selectedAssignee.name)}
            </AvatarFallback>
          </Avatar>
        ) : (
          <UserRound className="size-4" />
        )}
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
  // Trigger mirrors the row's priority icon: the CURRENT priority, not a
  // generic glyph (EXP-59).
  const priorityConfig = getIssuePriorityConfig(priority)
  const PriorityIcon = priorityConfig.icon

  return (
    <ContextMenuSub>
      <ContextMenuSubTrigger>
        <PriorityIcon className={`size-4 ${priorityConfig.color}`} />
        Priority
        <ContextMenuShortcut className={topLevelValueClass}>
          {priorityConfig.label}
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

interface ProjectSubmenuProps {
  projectId: Issue[`projectId`]
  // Workspace projects (sorted by name, trashed excluded upstream by the
  // projects shape). Kept a plain prop (no live query here) so this stays
  // presentational like its siblings.
  projects: Project[]
  topLevelValueClass: string
  onSelect: (projectId: string) => void
}

// EXP-57: move the issue to another project in the same workspace. The issue
// is renumbered in the target project (EXP-42 → ABC-17) server-side.
export function ProjectSubmenu({
  projectId,
  projects,
  topLevelValueClass,
  onSelect,
}: ProjectSubmenuProps) {
  const currentName = projects.find(
    (project) => project.id === projectId
  )?.name

  return (
    <ContextMenuSub>
      <ContextMenuSubTrigger>
        <FolderInput className="size-4" />
        Move to project
        <ContextMenuShortcut className={topLevelValueClass}>
          {currentName ?? `Project`}
        </ContextMenuShortcut>
      </ContextMenuSubTrigger>
      <ContextMenuSubContent className="w-[15rem]">
        <ContextMenuRadioGroup value={projectId}>
          {projects.length === 0 ? (
            <ContextMenuItem disabled inset>
              No projects yet
            </ContextMenuItem>
          ) : (
            projects.map((project) => (
              <ContextMenuRadioItem
                key={project.id}
                value={project.id}
                disabled={project.id === projectId}
                onSelect={() => {
                  if (project.id !== projectId) {
                    onSelect(project.id)
                  }
                }}
              >
                <div
                  className="size-2.5 rounded-full shrink-0"
                  style={{ backgroundColor: project.color }}
                />
                <span className="truncate">{project.name}</span>
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
