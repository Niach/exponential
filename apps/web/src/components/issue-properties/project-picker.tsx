import { useMemo, useState } from "react"
import { useLiveQuery, eq } from "@tanstack/react-db"
import { Check } from "lucide-react"
import { projectCollection } from "@/lib/collections"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "@/components/mobile-popover"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import { Button } from "@/components/ui/button"
import type { Project } from "@/db/schema"

interface ProjectPickerProps {
  disabled?: boolean
  workspaceId: string
  selectedProjectId: string
  onSelect: (projectId: string) => void | Promise<void>
}

// Move-to-project picker for the issue detail view (EXP-57): single-select
// over the workspace's projects (same workspace only; trashed projects never
// reach the client). Mirrors the release picker's MobilePopover + Command
// structure; picking the current project is a no-op. The server renumbers the
// issue in the target project (EXP-42 → ABC-17).
export function ProjectPicker({
  disabled,
  workspaceId,
  selectedProjectId,
  onSelect,
}: ProjectPickerProps) {
  const [open, setOpen] = useState(false)

  const { data: projectRows } = useLiveQuery(
    (q) =>
      workspaceId
        ? q
            .from({ projects: projectCollection })
            .where(({ projects }) => eq(projects.workspaceId, workspaceId))
        : undefined,
    [workspaceId]
  )

  const projects = useMemo(
    () =>
      [...((projectRows ?? []) as Project[])].sort((left, right) =>
        left.name.localeCompare(right.name)
      ),
    [projectRows]
  )
  const selectedProject =
    projects.find((project) => project.id === selectedProjectId) ?? null

  const handlePick = (projectId: string) => {
    setOpen(false)
    if (projectId !== selectedProjectId) {
      void onSelect(projectId)
    }
  }

  return (
    <MobilePopover
      open={disabled ? false : open}
      onOpenChange={(o) => {
        if (disabled) return
        setOpen(o)
      }}
    >
      <MobilePopoverTrigger asChild>
        <Button
          variant="ghost"
          size="xs"
          className="text-muted-foreground"
          disabled={disabled}
        >
          <div
            className="h-2.5 w-2.5 shrink-0 rounded-full"
            style={{ backgroundColor: selectedProject?.color ?? `#71717a` }}
          />
          {selectedProject ? (
            <span className="max-w-[7.5rem] truncate">
              {selectedProject.name}
            </span>
          ) : (
            `Project`
          )}
        </Button>
      </MobilePopoverTrigger>
      <MobilePopoverContent
        className="w-[14rem] p-0"
        align="start"
        mobileTitle="Move to project"
      >
        <Command>
          <CommandInput placeholder="Move to project..." />
          <CommandList>
            <CommandEmpty>No projects found.</CommandEmpty>
            <CommandGroup>
              {projects.map((project) => (
                <CommandItem
                  key={project.id}
                  value={project.name}
                  onSelect={() => handlePick(project.id)}
                  className="flex items-center gap-2"
                >
                  <div
                    className="h-2.5 w-2.5 shrink-0 rounded-full"
                    style={{ backgroundColor: project.color }}
                  />
                  <span className="min-w-0 truncate text-sm">
                    {project.name}
                  </span>
                  {project.id === selectedProjectId && (
                    <Check className="ml-auto size-3.5 shrink-0" />
                  )}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </MobilePopoverContent>
    </MobilePopover>
  )
}
