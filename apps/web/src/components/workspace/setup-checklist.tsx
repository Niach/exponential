import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { Check, Circle, Rocket, X } from "lucide-react"
import { useSetupChecklist } from "@/hooks/use-setup-checklist"
import { Button } from "@/components/ui/button"
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
} from "@/components/ui/sidebar"
import { cn } from "@/lib/utils"

type Step = {
  label: string
  done: boolean
  action: () => void
}

// Persistent, signal-backed "Set up coding agent" checklist. Reads real server
// signals (companion.setupStatus) so steps tick themselves as the user makes
// progress on web OR desktop. Dismissible; hidden once complete.
export function SetupChecklist({
  workspaceId,
  workspaceSlug,
  onCreateProject,
}: {
  workspaceId: string
  workspaceSlug: string
  onCreateProject: () => void
}) {
  const navigate = useNavigate()
  const { status, dismiss } = useSetupChecklist(workspaceId)
  const [expanded, setExpanded] = useState(true)

  if (!status || status.dismissed || status.allComplete) return null

  const goSetupAgent = () =>
    navigate({ to: `/w/$workspaceSlug/setup-agent`, params: { workspaceSlug } })

  const steps: Step[] = [
    { label: `Create a project`, done: status.hasProject, action: onCreateProject },
    { label: `Connect GitHub`, done: status.githubConnected, action: goSetupAgent },
    {
      label: `Register a desktop agent`,
      done: status.machineRegistered,
      action: goSetupAgent,
    },
    {
      label: `Assign an issue to the agent`,
      done: status.firstIssueAssignedToAgent,
      action: goSetupAgent,
    },
  ]
  const doneCount = steps.filter((s) => s.done).length

  return (
    <SidebarGroup>
      <SidebarGroupLabel className="flex items-center gap-1.5">
        <Rocket className="size-3.5" />
        <Button
          variant="ghost"
          onClick={() => setExpanded((v) => !v)}
          className="h-auto flex-1 justify-start p-0 text-xs font-medium text-muted-foreground hover:bg-transparent"
        >
          Setup {doneCount}/{steps.length}
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="h-5 w-5"
          aria-label="Dismiss setup checklist"
          onClick={dismiss}
        >
          <X className="size-3" />
        </Button>
      </SidebarGroupLabel>
      {expanded && (
        <SidebarGroupContent>
          <div className="space-y-0.5 px-2 py-1">
            {steps.map((step) => (
              <Button
                key={step.label}
                variant="ghost"
                onClick={step.done ? undefined : step.action}
                disabled={step.done}
                className={cn(
                  `flex h-auto w-full items-center justify-start gap-2 rounded-md px-1.5 py-1 text-left text-xs font-normal disabled:opacity-100`,
                  step.done ? `text-muted-foreground` : `text-foreground`
                )}
              >
                {step.done ? (
                  <Check className="size-3.5 shrink-0 text-green-500" />
                ) : (
                  <Circle className="size-3.5 shrink-0 text-muted-foreground" />
                )}
                <span className={cn(step.done && `line-through`)}>
                  {step.label}
                </span>
              </Button>
            ))}
          </div>
        </SidebarGroupContent>
      )}
    </SidebarGroup>
  )
}
